

#[postg::test]
async fn test_url_injection(url: String) {
    assert!(url.starts_with("postgresql://"));
    assert!(!url.is_empty());
}

#[postg::test]
async fn test_db_injection(db: postg::engine::Postg) {
    assert!(db.port() > 0);
    assert!(db.connection_string().starts_with("postgresql://"));
}

#[postg::test]
async fn test_no_params() {
    let x = 1 + 1;
    assert_eq!(x, 2);
}

#[postg::test(engine = "postgresql-spock")]
async fn test_spock_config(db: postg::engine::Postg) {
    // Should successfully start the spock engine variant
    assert!(db.port() > 0);
}
