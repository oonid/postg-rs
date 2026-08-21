// Type mapping and conversions between Arrow and PostgreSQL types

use arrow::datatypes::{DataType, TimeUnit};
use postgres_types::Type;
use anyhow::{anyhow, Result};

pub fn pg_type_enum_to_arrow(pg_type: &Type) -> Result<DataType> {
    match *pg_type {
        Type::INT2 => Ok(DataType::Int16),
        Type::INT4 => Ok(DataType::Int32),
        Type::INT8 => Ok(DataType::Int64),
        Type::FLOAT4 => Ok(DataType::Float32),
        Type::FLOAT8 => Ok(DataType::Float64),
        Type::BOOL => Ok(DataType::Boolean),
        Type::TEXT | Type::VARCHAR => Ok(DataType::LargeUtf8),
        Type::BYTEA => Ok(DataType::LargeBinary),
        Type::DATE => Ok(DataType::LargeUtf8), // Downgraded
        Type::TIME => Ok(DataType::LargeUtf8), // Downgraded
        Type::TIMESTAMP => Ok(DataType::LargeUtf8), // Downgraded
        Type::TIMESTAMPTZ => Ok(DataType::LargeUtf8), // Downgraded
        Type::NUMERIC => Ok(DataType::LargeUtf8), // Downgraded
        Type::UUID => Ok(DataType::LargeUtf8), // Mapped to string for broader compatibility
        Type::JSON | Type::JSONB => Ok(DataType::LargeUtf8),
        _ => Err(anyhow!("Unsupported PostgreSQL type OID: {}", pg_type.oid())),
    }
}

pub fn pg_type_to_arrow(oid: u32) -> Result<DataType> {
    let pg_type = Type::from_oid(oid).ok_or_else(|| anyhow!("Unsupported PostgreSQL type OID: {}", oid))?;
    pg_type_enum_to_arrow(&pg_type)
}

pub fn pg_oid_to_arrow(oid: u32) -> Result<DataType> {
    pg_type_to_arrow(oid)
}

pub fn arrow_to_pg_type(dt: &DataType) -> Result<String> {
    match dt {
        DataType::Int16 => Ok("INT2".to_string()),
        DataType::Int32 => Ok("INT4".to_string()),
        DataType::Int64 => Ok("INT8".to_string()),
        DataType::Float32 => Ok("FLOAT4".to_string()),
        DataType::Float64 => Ok("FLOAT8".to_string()),
        DataType::Boolean => Ok("BOOL".to_string()),
        DataType::Utf8 | DataType::LargeUtf8 => Ok("TEXT".to_string()),
        DataType::Binary | DataType::LargeBinary => Ok("BYTEA".to_string()),
        DataType::Date32 => Ok("DATE".to_string()),
        DataType::Time64(_) => Ok("TIME".to_string()),
        DataType::Timestamp(_, None) => Ok("TIMESTAMP".to_string()),
        DataType::Timestamp(_, Some(_)) => Ok("TIMESTAMPTZ".to_string()),
        DataType::Decimal128(_, _) => Ok("NUMERIC".to_string()),
        _ => Err(anyhow!("Unsupported Arrow data type for PostgreSQL mapping: {:?}", dt)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pg_type_enum_to_arrow_primitives() {
        assert_eq!(pg_type_enum_to_arrow(&Type::INT2).unwrap(), DataType::Int16);
        assert_eq!(pg_type_enum_to_arrow(&Type::INT4).unwrap(), DataType::Int32);
        assert_eq!(pg_type_enum_to_arrow(&Type::INT8).unwrap(), DataType::Int64);
        assert_eq!(pg_type_enum_to_arrow(&Type::FLOAT4).unwrap(), DataType::Float32);
        assert_eq!(pg_type_enum_to_arrow(&Type::FLOAT8).unwrap(), DataType::Float64);
        assert_eq!(pg_type_enum_to_arrow(&Type::BOOL).unwrap(), DataType::Boolean);
    }

    #[test]
    fn test_pg_type_enum_to_arrow_strings_and_binary() {
        assert_eq!(pg_type_enum_to_arrow(&Type::TEXT).unwrap(), DataType::LargeUtf8);
        assert_eq!(pg_type_enum_to_arrow(&Type::VARCHAR).unwrap(), DataType::LargeUtf8);
        assert_eq!(pg_type_enum_to_arrow(&Type::BYTEA).unwrap(), DataType::LargeBinary);
    }

    #[test]
    fn test_pg_type_enum_to_arrow_temporal_and_complex() {
        assert_eq!(pg_type_enum_to_arrow(&Type::DATE).unwrap(), DataType::LargeUtf8);
        assert_eq!(
            pg_type_enum_to_arrow(&Type::TIME).unwrap(),
            DataType::LargeUtf8
        );
        assert_eq!(
            pg_type_enum_to_arrow(&Type::TIMESTAMP).unwrap(),
            DataType::LargeUtf8
        );
        assert_eq!(
            pg_type_enum_to_arrow(&Type::TIMESTAMPTZ).unwrap(),
            DataType::LargeUtf8
        );
        assert_eq!(
            pg_type_enum_to_arrow(&Type::NUMERIC).unwrap(),
            DataType::LargeUtf8
        );
        assert_eq!(pg_type_enum_to_arrow(&Type::UUID).unwrap(), DataType::LargeUtf8);
        assert_eq!(pg_type_enum_to_arrow(&Type::JSON).unwrap(), DataType::LargeUtf8);
        assert_eq!(pg_type_enum_to_arrow(&Type::JSONB).unwrap(), DataType::LargeUtf8);
    }

    #[test]
    fn test_pg_type_enum_to_arrow_unsupported() {
        assert!(pg_type_enum_to_arrow(&Type::POINT).is_err());
        assert!(pg_type_enum_to_arrow(&Type::MONEY).is_err());
    }

    #[test]
    fn test_pg_type_to_arrow_and_oid_alias() {
        assert_eq!(pg_type_to_arrow(23).unwrap(), DataType::Int32); // INT4 OID is 23
        assert_eq!(pg_oid_to_arrow(23).unwrap(), DataType::Int32);
        assert!(pg_type_to_arrow(999999).is_err());
        assert!(pg_oid_to_arrow(999999).is_err());
    }

    #[test]
    fn test_arrow_to_pg_type_primitives() {
        assert_eq!(arrow_to_pg_type(&DataType::Int16).unwrap(), "INT2");
        assert_eq!(arrow_to_pg_type(&DataType::Int32).unwrap(), "INT4");
        assert_eq!(arrow_to_pg_type(&DataType::Int64).unwrap(), "INT8");
        assert_eq!(arrow_to_pg_type(&DataType::Float32).unwrap(), "FLOAT4");
        assert_eq!(arrow_to_pg_type(&DataType::Float64).unwrap(), "FLOAT8");
        assert_eq!(arrow_to_pg_type(&DataType::Boolean).unwrap(), "BOOL");
    }

    #[test]
    fn test_arrow_to_pg_type_strings_and_binary() {
        assert_eq!(arrow_to_pg_type(&DataType::Utf8).unwrap(), "TEXT");
        assert_eq!(arrow_to_pg_type(&DataType::LargeUtf8).unwrap(), "TEXT");
        assert_eq!(arrow_to_pg_type(&DataType::Binary).unwrap(), "BYTEA");
        assert_eq!(arrow_to_pg_type(&DataType::LargeBinary).unwrap(), "BYTEA");
    }

    #[test]
    fn test_arrow_to_pg_type_temporal_and_complex() {
        assert_eq!(arrow_to_pg_type(&DataType::Date32).unwrap(), "DATE");
        assert_eq!(
            arrow_to_pg_type(&DataType::Time64(TimeUnit::Microsecond)).unwrap(),
            "TIME"
        );
        assert_eq!(
            arrow_to_pg_type(&DataType::Timestamp(TimeUnit::Microsecond, None)).unwrap(),
            "TIMESTAMP"
        );
        assert_eq!(
            arrow_to_pg_type(&DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))).unwrap(),
            "TIMESTAMPTZ"
        );
        assert_eq!(
            arrow_to_pg_type(&DataType::Decimal128(38, 9)).unwrap(),
            "NUMERIC"
        );
    }

    #[test]
    fn test_arrow_to_pg_type_unsupported() {
        assert!(arrow_to_pg_type(&DataType::Null).is_err());
        assert!(arrow_to_pg_type(&DataType::Float16).is_err());
        assert!(arrow_to_pg_type(&DataType::UInt32).is_err());
    }
}
