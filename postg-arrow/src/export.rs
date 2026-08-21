use crate::types::pg_oid_to_arrow;
use arrow::array::{ArrayRef, Int16Builder, Int32Builder, Int64Builder, Float32Builder, Float64Builder, BooleanBuilder, LargeBinaryBuilder, LargeStringBuilder};
use arrow::datatypes::{Field, Schema, DataType};
use arrow::record_batch::RecordBatch;
use bytes::{Buf, BytesMut};
use futures::{Stream, StreamExt};
use sqlx::{PgConnection, Executor, Column, Statement, TypeInfo};
use sqlx::postgres::PgTypeInfo;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::convert::TryInto;

enum ColumnBuilder {
    Int16(Int16Builder),
    Int32(Int32Builder),
    Int64(Int64Builder),
    Float32(Float32Builder),
    Float64(Float64Builder),
    Boolean(BooleanBuilder),
    LargeBinary(LargeBinaryBuilder),
    LargeUtf8(LargeStringBuilder),
}

impl ColumnBuilder {
    fn append_null(&mut self) {
        match self {
            ColumnBuilder::Int16(b) => b.append_null(),
            ColumnBuilder::Int32(b) => b.append_null(),
            ColumnBuilder::Int64(b) => b.append_null(),
            ColumnBuilder::Float32(b) => b.append_null(),
            ColumnBuilder::Float64(b) => b.append_null(),
            ColumnBuilder::Boolean(b) => b.append_null(),
            ColumnBuilder::LargeBinary(b) => b.append_null(),
            ColumnBuilder::LargeUtf8(b) => b.append_null(),
        }
    }

    fn finish(&mut self) -> ArrayRef {
        match self {
            ColumnBuilder::Int16(b) => Arc::new(b.finish()),
            ColumnBuilder::Int32(b) => Arc::new(b.finish()),
            ColumnBuilder::Int64(b) => Arc::new(b.finish()),
            ColumnBuilder::Float32(b) => Arc::new(b.finish()),
            ColumnBuilder::Float64(b) => Arc::new(b.finish()),
            ColumnBuilder::Boolean(b) => Arc::new(b.finish()),
            ColumnBuilder::LargeBinary(b) => Arc::new(b.finish()),
            ColumnBuilder::LargeUtf8(b) => Arc::new(b.finish()),
        }
    }

    fn new_from_datatype(dt: &DataType) -> Result<Self> {
        match dt {
            DataType::Int16 => Ok(ColumnBuilder::Int16(Int16Builder::with_capacity(1000))),
            DataType::Int32 => Ok(ColumnBuilder::Int32(Int32Builder::with_capacity(1000))),
            DataType::Int64 => Ok(ColumnBuilder::Int64(Int64Builder::with_capacity(1000))),
            DataType::Float32 => Ok(ColumnBuilder::Float32(Float32Builder::with_capacity(1000))),
            DataType::Float64 => Ok(ColumnBuilder::Float64(Float64Builder::with_capacity(1000))),
            DataType::Boolean => Ok(ColumnBuilder::Boolean(BooleanBuilder::with_capacity(1000))),
            DataType::LargeBinary => Ok(ColumnBuilder::LargeBinary(LargeBinaryBuilder::with_capacity(1000, 1024))),
            DataType::LargeUtf8 => Ok(ColumnBuilder::LargeUtf8(LargeStringBuilder::with_capacity(1000, 1024))),
            _ => Err(anyhow!("Unsupported type in export: {:?}", dt)),
        }
    }
}

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

        let mut builders: Vec<ColumnBuilder> = Vec::new();
        for f in schema_clone.fields() {
            builders.push(ColumnBuilder::new_from_datatype(f.data_type())?);
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
                        builders[i].append_null();
                    } else {
                        let data = buf.split_to(col_len as usize);
                        match &mut builders[i] {
                            ColumnBuilder::Int16(b) => {
                                b.append_value(i16::from_be_bytes(data[..2].try_into().unwrap()));
                            }
                            ColumnBuilder::Int32(b) => {
                                b.append_value(i32::from_be_bytes(data[..4].try_into().unwrap()));
                            }
                            ColumnBuilder::Int64(b) => {
                                b.append_value(i64::from_be_bytes(data[..8].try_into().unwrap()));
                            }
                            ColumnBuilder::Float32(b) => {
                                b.append_value(f32::from_be_bytes(data[..4].try_into().unwrap()));
                            }
                            ColumnBuilder::Float64(b) => {
                                b.append_value(f64::from_be_bytes(data[..8].try_into().unwrap()));
                            }
                            ColumnBuilder::Boolean(b) => {
                                b.append_value(data[0] != 0);
                            }
                            ColumnBuilder::LargeBinary(b) => {
                                b.append_value(&data);
                            }
                            ColumnBuilder::LargeUtf8(b) => {
                                let val = std::str::from_utf8(&data)?;
                                b.append_value(val);
                            }
                        }
                    }
                }
                
                row_count += 1;
                if row_count >= 1000 {
                    let mut arrays: Vec<ArrayRef> = Vec::new();
                    for i in 0..num_cols as usize {
                        arrays.push(builders[i].finish());
                        builders[i] = ColumnBuilder::new_from_datatype(schema_clone.field(i).data_type())?;
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
                arrays.push(builders[i].finish());
            }
            let batch = RecordBatch::try_new(schema_clone.clone(), arrays)?;
            yield batch;
        }
    };

    Ok((schema, stream))
}
