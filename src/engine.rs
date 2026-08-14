use crate::config::{Config, Engine as EngineVariant};
use crate::error::{Error, Result};
use std::fs;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const MANAGED_CONF_BEGIN: &str = "# --- postg managed settings begin ---";
const MANAGED_CONF_END: &str = "# --- postg managed settings end ---";

pub struct Postg {
    config: Config,
    child: Option<Child>,
}

impl Postg {
    pub async fn start(mut config: Config) -> Result<Self> {
        crate::payload::extract_payload(&config, None).await?;

        // Resolve ephemeral port
        if config.port == 0 {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;
            config.port = listener.local_addr()?.port();
        }

        // Ensure data directory exists
        fs::create_dir_all(&config.data_dir)?;

        // Run initdb if not already initialized
        if !config.data_dir.join("PG_VERSION").exists() {
            Self::run_initdb(&config).await?;
        }

        // Write/update postgresql.conf
        Self::write_postgresql_conf(&config)?;

        // Write pg_hba.conf for trust auth on localhost
        Self::write_pg_hba_conf(&config)?;

        // Start postgres
        let child = Self::spawn_postgres(&config)?;

        let instance = Postg {
            config,
            child: Some(child),
        };

        // Wait for ready
        instance.wait_for_ready().await?;

        Ok(instance)
    }

    pub fn connection_string(&self) -> String {
        self.config.connection_string()
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    async fn run_initdb(config: &Config) -> Result<()> {
        tracing::info!("running initdb on {}", config.data_dir.display());
        let output = Command::new(config.pg_bin("initdb"))
            .args([
                "--auth=trust",
                "--encoding=UTF8",
                "-U",
                &config.username,
                "-D",
            ])
            .arg(&config.data_dir)
            .env("TZ", "UTC")
            .output()?;

        if !output.status.success() {
            return Err(Error::InitDb(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        tracing::info!("initdb completed");
        Ok(())
    }

    fn write_postgresql_conf(config: &Config) -> Result<()> {
        let mut managed = String::new();
        managed.push_str(&format!("port = {}\n", config.port));
        managed.push_str(&format!("listen_addresses = '{}'\n", config.host));
        managed.push_str("unix_socket_directories = ''\n");
        managed.push_str("wal_level = logical\n");

        if config.engine == EngineVariant::Spock {
            managed.push_str("max_worker_processes = 10\n");
            managed.push_str("max_replication_slots = 10\n");
            managed.push_str("max_wal_senders = 10\n");
            managed.push_str("shared_preload_libraries = 'spock'\n");
            managed.push_str("output_plugin_libraries = 'spock_output'\n");
            managed.push_str("track_commit_timestamp = on\n");
            managed.push_str("spock.conflict_resolution = 'last_update_wins'\n");
            managed.push_str("spock.enable_ddl_replication = on\n");
            managed.push_str("spock.include_ddl_repset = on\n");
        } else {
            managed.push_str("max_replication_slots = 4\n");
            managed.push_str("max_wal_senders = 4\n");
        }

        let conf_path = config.data_dir.join("postgresql.conf");
        let existing = fs::read_to_string(&conf_path).unwrap_or_default();

        // Replace the managed section if it exists, otherwise append
        let new_content = if let Some(begin_pos) = existing.find(MANAGED_CONF_BEGIN) {
            let before = &existing[..begin_pos];
            let after = existing
                .find(MANAGED_CONF_END)
                .map(|pos| &existing[pos + MANAGED_CONF_END.len()..])
                .unwrap_or("");
            format!(
                "{}{}\n{}{}\n{}",
                before, MANAGED_CONF_BEGIN, managed, MANAGED_CONF_END, after
            )
        } else {
            format!(
                "{}\n{}\n{}{}\n",
                existing, MANAGED_CONF_BEGIN, managed, MANAGED_CONF_END
            )
        };

        fs::write(&conf_path, new_content)?;
        Ok(())
    }

    fn write_pg_hba_conf(config: &Config) -> Result<()> {
        let hba = "# postg: trust all local connections\n\
                    local all all trust\n\
                    host all all 127.0.0.1/32 trust\n\
                    host all all ::1/128 trust\n\
                    host replication all 127.0.0.1/32 trust\n\
                    host replication all ::1/128 trust\n";
        fs::write(config.data_dir.join("pg_hba.conf"), hba)?;
        Ok(())
    }

    fn spawn_postgres(config: &Config) -> Result<Child> {
        tracing::info!("starting postgres on port {}", config.port);
        let log_file = fs::File::create(config.data_dir.join("postgres.log"))
            .map_err(|e| Error::Start(format!("failed to create log file: {e}")))?;
            
        let child = Command::new(config.pg_bin("postgres"))
            .arg("-D")
            .arg(&config.data_dir)
            .stdout(log_file.try_clone().map_err(|e| Error::Start(e.to_string()))?)
            .stderr(log_file)
            .spawn()
            .map_err(|e| Error::Start(format!("failed to spawn postgres: {e}")))?;
        Ok(child)
    }

    async fn wait_for_ready(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err(Error::Start("postgres did not become ready in 30s".into()));
            }
            match tokio::net::TcpStream::connect((&*self.config.host, self.config.port)).await {
                Ok(_) => {
                    tracing::info!("postgres ready on port {}", self.config.port);
                    return Ok(());
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            tracing::info!("stopping postgres (pid {})", child.id());
            // Send pg_ctl stop
            let output = Command::new(self.config.pg_bin("pg_ctl"))
                .args(["stop", "-D"])
                .arg(&self.config.data_dir)
                .args(["-m", "fast", "-w"])
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    tracing::info!("postgres stopped gracefully");
                }
                _ => {
                    // Fallback: kill the child
                    let _ = child.kill();
                    let _ = child.wait();
                    tracing::warn!("postgres killed forcefully");
                }
            }
            self.child = None;
        }

        if self.config.temporary && self.config.data_dir.exists() {
            let _ = fs::remove_dir_all(&self.config.data_dir);
            tracing::info!("temporary data dir removed");
            self.config.temporary = false;
        }

        Ok(())
    }
}

impl Drop for Postg {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Use .output() which waits for pg_ctl to finish, giving postgres
            // a chance to shut down gracefully before we resort to kill.
            let pg_ctl_result = Command::new(self.config.pg_bin("pg_ctl"))
                .args(["stop", "-D"])
                .arg(&self.config.data_dir)
                .args(["-m", "fast", "-w"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output();

            match pg_ctl_result {
                Ok(o) if o.status.success() => {
                    // pg_ctl handled the shutdown; wait for our child handle to reap.
                    let _ = child.wait();
                }
                _ => {
                    // pg_ctl failed or wasn't available — forceful kill.
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        if self.config.temporary && self.config.data_dir.exists() {
            let _ = fs::remove_dir_all(&self.config.data_dir);
        }
    }
}
