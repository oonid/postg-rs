use sqlx::{PgConnection, Connection};
use futures::StreamExt;
async fn test(conn: &mut PgConnection) {
    let mut out = conn.copy_out_raw("").await.unwrap();
    while let Some(chunk) = out.next().await {
        let x: bytes::Bytes = chunk.unwrap();
    }
}
