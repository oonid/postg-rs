use sqlx::PgConnection;
async fn test(conn: &mut PgConnection) {
    let mut sink = conn.copy_in_raw("").await.unwrap();
    sink.send(bytes::Bytes::from("hello")).await.unwrap();
}
