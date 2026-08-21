use crate::types::arrow_to_pg_type;
use arrow::datatypes::{Schema, DataType};
use arrow::record_batch::RecordBatch;
use arrow::array::{Int32Array, LargeStringArray};
use bytes::{BufMut, BytesMut};
use futures::{Stream, StreamExt};
use sqlx::{PgConnection, Executor};
use anyhow::{anyhow, Result};

/// Generates a PostgreSQL CREATE TABLE DDL statement from an Arrow Schema.
pub fn generate_create_table_ddl(table: &str, schema: &Schema) -> Result<String> {
    let mut column_defs = Vec::new();
    for field in schema.fields() {
        let pg_type = arrow_to_pg_type(field.data_type())?;
        let not_null = if field.is_nullable() { "" } else { " NOT NULL" };
        column_defs.push(format!("\"{}\" {}{}", field.name(), pg_type, not_null));
    }
    let ddl = format!("CREATE TABLE \"{}\" ({});", table, column_defs.join(", "));
    Ok(ddl)
}

/// Import an Arrow RecordBatch stream into a PostgreSQL table.
pub async fn arrow_to_table(
    conn: &mut PgConnection,
    table: &str,
    create_table: bool,
    schema: &Schema,
    mut stream: impl Stream<Item = Result<RecordBatch>> + Unpin,
) -> Result<()> {
    if create_table {
        let ddl = generate_create_table_ddl(table, schema)?;
        conn.execute(ddl.as_str()).await?;
    }

    let copy_query = format!("COPY \"{}\" FROM STDIN WITH (FORMAT binary)", table);
    let mut copy_in = conn.copy_in_raw(copy_query.as_str()).await?;

    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"PGCOPY\n\xff\r\n\0");
    buf.put_i32(0);
    buf.put_i32(0);
    copy_in.send(buf.split().freeze()).await?;

    let num_cols = schema.fields().len();

    while let Some(batch) = stream.next().await {
        let batch = batch?;
        let rows = batch.num_rows();

        for r in 0..rows {
            buf.put_i16(num_cols as i16);
            for c in 0..num_cols {
                let col = batch.column(c);
                if col.is_null(r) {
                    buf.put_i32(-1);
                } else {
                    match schema.field(c).data_type() {
                        DataType::Int32 => {
                            let arr = col.as_any().downcast_ref::<Int32Array>().unwrap();
                            buf.put_i32(4);
                            buf.put_i32(arr.value(r));
                        }
                        DataType::LargeUtf8 => {
                            let arr = col.as_any().downcast_ref::<LargeStringArray>().unwrap();
                            let val = arr.value(r);
                            buf.put_i32(val.len() as i32);
                            buf.extend_from_slice(val.as_bytes());
                        }
                        DataType::Utf8 => {
                            let arr = col.as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
                            let val = arr.value(r);
                            buf.put_i32(val.len() as i32);
                            buf.extend_from_slice(val.as_bytes());
                        }
                        _ => {
                            return Err(anyhow!("Unsupported type in import: {:?}", schema.field(c).data_type()));
                        }
                    }
                }
            }
            if buf.len() > 8192 {
                copy_in.send(buf.split().freeze()).await?;
            }
        }
    }

    buf.put_i16(-1);
    copy_in.send(buf.split().freeze()).await?;
    copy_in.finish().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, TimeUnit};

    #[test]
    fn test_generate_create_table_ddl_basic() {
        let fields = vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("score", DataType::Float64, true),
            Field::new("active", DataType::Boolean, false),
        ];
        let schema = Schema::new(fields);
        let ddl = generate_create_table_ddl("users", &schema).unwrap();
        assert_eq!(
            ddl,
            "CREATE TABLE \"users\" (\"id\" INT4 NOT NULL, \"name\" TEXT, \"score\" FLOAT8, \"active\" BOOL NOT NULL);"
        );
    }

    #[test]
    fn test_generate_create_table_ddl_all_types() {
        let fields = vec![
            Field::new("col_i16", DataType::Int16, false),
            Field::new("col_i32", DataType::Int32, false),
            Field::new("col_i64", DataType::Int64, false),
            Field::new("col_f32", DataType::Float32, true),
            Field::new("col_f64", DataType::Float64, true),
            Field::new("col_bool", DataType::Boolean, false),
            Field::new("col_utf8", DataType::Utf8, true),
            Field::new("col_large_utf8", DataType::LargeUtf8, true),
            Field::new("col_bin", DataType::Binary, true),
            Field::new("col_large_bin", DataType::LargeBinary, true),
            Field::new("col_date", DataType::Date32, true),
            Field::new("col_time", DataType::Time64(TimeUnit::Microsecond), true),
            Field::new("col_ts", DataType::Timestamp(TimeUnit::Microsecond, None), true),
            Field::new(
                "col_tstz",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Field::new("col_decimal", DataType::Decimal128(38, 9), true),
        ];
        let schema = Schema::new(fields);
        let ddl = generate_create_table_ddl("all_types", &schema).unwrap();
        assert_eq!(
            ddl,
            "CREATE TABLE \"all_types\" (\
             \"col_i16\" INT2 NOT NULL, \
             \"col_i32\" INT4 NOT NULL, \
             \"col_i64\" INT8 NOT NULL, \
             \"col_f32\" FLOAT4, \
             \"col_f64\" FLOAT8, \
             \"col_bool\" BOOL NOT NULL, \
             \"col_utf8\" TEXT, \
             \"col_large_utf8\" TEXT, \
             \"col_bin\" BYTEA, \
             \"col_large_bin\" BYTEA, \
             \"col_date\" DATE, \
             \"col_time\" TIME, \
             \"col_ts\" TIMESTAMP, \
             \"col_tstz\" TIMESTAMPTZ, \
             \"col_decimal\" NUMERIC);"
        );
    }

    #[test]
    fn test_generate_create_table_ddl_unsupported_type() {
        let fields = vec![Field::new("invalid_col", DataType::Null, true)];
        let schema = Schema::new(fields);
        let err = generate_create_table_ddl("invalid_table", &schema);
        assert!(err.is_err());
    }

    #[test]
    fn test_import_interface_signature() {
        // Verify type signature and Stream Item conformance
        fn _assert_signature<'a, S>(_stream: S)
        where
            S: Stream<Item = Result<RecordBatch>> + Unpin + 'a,
        {
            let _ = |conn: &'a mut PgConnection,
                     table: &'a str,
                     create_table: bool,
                     schema: &'a Schema,
                     stream: S| {
                arrow_to_table(conn, table, create_table, schema, stream)
            };
        }

        _assert_signature(futures::stream::empty::<Result<RecordBatch>>());
    }
}
