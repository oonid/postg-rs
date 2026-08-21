use crate::types::pg_oid_to_arrow;
use arrow::array::{ArrayRef, Int32Builder, LargeStringBuilder};
use arrow::datatypes::{Field, Schema, DataType};
use arrow::record_batch::RecordBatch;
use bytes::{Buf, BytesMut};
use futures::{Stream, StreamExt};
use sqlx::{PgConnection, Executor, Column, Statement, TypeInfo};
use sqlx::postgres::PgTypeInfo;
use anyhow::{anyhow, Result};
use std::sync::Arc;

pub async fn query_to_arrow<'a>(
    conn: &'a mut PgConnection,
    query: &str,
) -> Result<(Arc<Schema>, impl Stream<Item = Result<RecordBatch>> + 'a)> {
    let describe_query = format!("SELECT * FROM ({}) AS _t LIMIT 0", query);
    let stmt = conn.prepare(describe_query.as_str()).await?;

    let mut fields: Vec<Field> = Vec::new();
    for col in stmt.columns() {
        let type_info: &PgTypeInfo = col.type_info();
        let oid: u32 = type_info.oid().map(|o| o.0).unwrap_or(0);
        let arrow_type = pg_oid_to_arrow(oid)?;
        fields.push(Field::new(col.name(), arrow_type, true));
    }
    let schema = Arc::new(Schema::new(fields));

    let copy_query = format!("COPY ({}) TO STDOUT WITH (FORMAT binary)", query);
    let mut copy_out = conn.copy_out_raw(copy_query.as_str()).await?;

    let schema_clone = schema.clone();
    
    let stream = async_stream::try_stream! {
        let mut buf = BytesMut::new();
        let mut header_parsed = false;

        let mut int32_builders: Vec<Option<Int32Builder>> = Vec::new();
        let mut text_builders: Vec<Option<LargeStringBuilder>> = Vec::new();
        for f in schema_clone.fields() {
            match f.data_type() {
                DataType::Int32 => {
                    int32_builders.push(Some(Int32Builder::with_capacity(1000)));
                    text_builders.push(None);
                }
                DataType::LargeUtf8 => {
                    int32_builders.push(None);
                    text_builders.push(Some(LargeStringBuilder::with_capacity(1000, 1024)));
                }
                _ => {
                    Err(anyhow!("Unsupported type in export: {:?}", f.data_type()))?;
                }
            }
        }

        let mut row_count = 0;

        while let Some(chunk_res) = copy_out.next().await {
            let chunk: bytes::Bytes = chunk_res?;
            buf.extend_from_slice(&chunk);

            if !header_parsed {
                if buf.len() >= 19 {
                    buf.advance(11); // Signature
                    let flags = buf.get_i32();
                    let ext_len = buf.get_i32();
                    buf.advance(ext_len as usize);
                    header_parsed = true;
                } else {
                    continue;
                }
            }

            loop {
                if buf.len() < 2 {
                    break;
                }
                let num_cols = i16::from_be_bytes([buf[0], buf[1]]);
                if num_cols == -1 {
                    buf.advance(2); // Trailer
                    break;
                }

                let mut offset = 2;
                let mut can_read = true;
                for _ in 0..num_cols {
                    if offset + 4 > buf.len() {
                        can_read = false;
                        break;
                    }
                    let col_len = i32::from_be_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]);
                    offset += 4;
                    if col_len > 0 {
                        if offset + col_len as usize > buf.len() {
                            can_read = false;
                            break;
                        }
                        offset += col_len as usize;
                    }
                }

                if !can_read {
                    break;
                }

                buf.advance(2); // num_cols
                for i in 0..num_cols as usize {
                    let col_len = buf.get_i32();
                    if col_len == -1 {
                        if let Some(b) = &mut int32_builders[i] {
                            b.append_null();
                        } else if let Some(b) = &mut text_builders[i] {
                            b.append_null();
                        }
                    } else {
                        let data = buf.split_to(col_len as usize);
                        if let Some(b) = &mut int32_builders[i] {
                            let val = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                            b.append_value(val);
                        } else if let Some(b) = &mut text_builders[i] {
                            let val = std::str::from_utf8(&data)?;
                            b.append_value(val);
                        }
                    }
                }
                
                row_count += 1;
                if row_count >= 1000 {
                    let mut arrays: Vec<ArrayRef> = Vec::new();
                    for i in 0..num_cols as usize {
                        if let Some(b) = &mut int32_builders[i] {
                            arrays.push(Arc::new(b.finish()));
                            *b = Int32Builder::with_capacity(1000);
                        } else if let Some(b) = &mut text_builders[i] {
                            arrays.push(Arc::new(b.finish()));
                            *b = LargeStringBuilder::with_capacity(1000, 1024);
                        }
                    }
                    let batch = RecordBatch::try_new(schema_clone.clone(), arrays)?;
                    yield batch;
                    row_count = 0;
                }
            }
        }

        if row_count > 0 {
            let mut arrays: Vec<ArrayRef> = Vec::new();
            for i in 0..schema_clone.fields().len() {
                if let Some(b) = &mut int32_builders[i] {
                    arrays.push(Arc::new(b.finish()));
                } else if let Some(b) = &mut text_builders[i] {
                    arrays.push(Arc::new(b.finish()));
                }
            }
            let batch = RecordBatch::try_new(schema_clone.clone(), arrays)?;
            yield batch;
        }
    };

    Ok((schema, stream))
}
