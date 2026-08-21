use sqlx::{PgConnection, Executor, Row, Column};
use sqlx::postgres::PgTypeInfo;
use sqlx::TypeInfo;

async fn test(conn: &mut PgConnection) {
    let stmt = conn.prepare("SELECT 1 AS a").await.unwrap();
    for col in stmt.columns() {
        let name = col.name();
        let oid = col.type_info().clone();
        // what is the oid accessor?
    }
}

fn main() {}
