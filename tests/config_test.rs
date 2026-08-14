use postg::config::{Config, Engine};

#[test]
fn default_config_uses_vanilla_engine() {
    let config = Config::default();
    assert_eq!(config.engine, Engine::Vanilla);
    assert_eq!(config.host, "127.0.0.1");
    assert!(config.temporary);
}

#[test]
fn connection_string_format() {
    let config = Config {
        port: 5432,
        ..Config::default()
    };
    assert_eq!(
        config.connection_string(),
        "postgresql://postgres@127.0.0.1:5432/postgres"
    );
}
