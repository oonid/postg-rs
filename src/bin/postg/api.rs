use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

#[derive(Serialize)]
pub struct TableMeta {
    pub name: String,
    pub bytes: i64,
    pub live_rows_estimate: i64,
}

#[derive(Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

pub fn app(pool: PgPool) -> Router {
    let state = AppState { pool };
    Router::new()
        .route("/meta/tables", get(get_tables))
        .route("/query", post(execute_query))
        .with_state(state)
}

async fn get_tables(
    State(state): State<AppState>,
) -> Result<Json<Vec<TableMeta>>, (StatusCode, String)> {
    // Inspired by postgres-meta, we query pg_catalog for table metadata.
    let q = r#"
        SELECT
            c.relname AS name,
            pg_total_relation_size(c.oid) AS bytes,
            pg_stat_get_live_tuples(c.oid) AS live_rows_estimate
        FROM
            pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE
            n.nspname = 'public'
            AND c.relkind = 'r'
    "#;

    let records = sqlx::query_as::<_, (String, i64, i64)>(q)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tables = records
        .into_iter()
        .map(|(name, bytes, live_rows_estimate)| TableMeta {
            name,
            bytes,
            live_rows_estimate,
        })
        .collect();

    Ok(Json(tables))
}

async fn execute_query(
    State(state): State<AppState>,
    Json(payload): Json<QueryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let query_upper = payload.query.trim().to_uppercase();
    let is_select = query_upper.starts_with("SELECT")
        || query_upper.starts_with("WITH")
        || query_upper.starts_with("EXPLAIN");

    if is_select {
        // A neat trick to easily serialize arbitrary SQL rows to JSON without writing a complex row parser.
        // We let PostgreSQL itself serialize the results to JSON.
        let wrapped_query = format!("SELECT json_agg(t) as result FROM ({}) as t", payload.query);
        let row: (Option<serde_json::Value>,) = sqlx::query_as(&wrapped_query)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(Json(row.0.unwrap_or(serde_json::json!([]))))
    } else {
        // For INSERT/UPDATE/DELETE/CREATE
        let result = sqlx::query(&payload.query)
            .execute(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(Json(serde_json::json!({
            "rows_affected": result.rows_affected()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use axum::extract::State;
    use axum::Json;
    use axum::http::StatusCode;

    #[postg::test]
    async fn test_get_tables(db: Postg) {
        let pool = sqlx::PgPool::connect(&db.connection_string()).await.unwrap();
        sqlx::query("CREATE TABLE api_test_table (id SERIAL PRIMARY KEY, val TEXT)").execute(&pool).await.unwrap();
        
        let state = State(AppState { pool });
        let result = get_tables(state).await.unwrap();
        let tables = result.0;
        
        let found = tables.iter().find(|t| t.name == "api_test_table");
        assert!(found.is_some());
    }

    #[postg::test]
    async fn test_execute_query(db: Postg) {
        let pool = sqlx::PgPool::connect(&db.connection_string()).await.unwrap();
        let state = State(AppState { pool });
        
        // Test CREATE TABLE
        let req = QueryRequest { query: "CREATE TABLE execute_test (id INT)".to_string() };
        let res = execute_query(state.clone(), Json(req)).await.unwrap();
        assert_eq!(res.0, serde_json::json!({"rows_affected": 0}));

        // Test INSERT
        let req = QueryRequest { query: "INSERT INTO execute_test (id) VALUES (42)".to_string() };
        let res = execute_query(state.clone(), Json(req)).await.unwrap();
        assert_eq!(res.0, serde_json::json!({"rows_affected": 1}));

        // Test SELECT
        let req = QueryRequest { query: "SELECT * FROM execute_test".to_string() };
        let res = execute_query(state.clone(), Json(req)).await.unwrap();
        let rows = res.0.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], 42);

        // Test syntax error (Select)
        let req = QueryRequest { query: "SELECT * FRO oops".to_string() };
        let err = execute_query(state.clone(), Json(req)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);

        // Test syntax error (Non-select)
        let req = QueryRequest { query: "INSERT INTO nonexistent VALUES (1)".to_string() };
        let err = execute_query(state, Json(req)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
