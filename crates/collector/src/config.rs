use std::{fmt::Display, ops::Deref, str::FromStr};

use thiserror::Error;
use tokio::sync::OnceCell;

pub async fn config() -> &'static Config {
    static INSTANCE: OnceCell<Config> = OnceCell::const_new();
    INSTANCE.get_or_init(|| async { Config::new() }).await
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Bad Config Value: {0}")]
    BadConfigValue(String),
    #[error("Missing Config Value")]
    MissingConfigValue,
}

pub struct Config {
    pub web_url: String,
    pub proc_dir: String,
    pub hostname_path: String,
    pub server_port: Port,
}

impl Config {
    fn new() -> Self {
        Self {
            web_url: std::env::var("COLLECTOR_WEB_URL")
                .expect("the environment variable COLLECTOR_WEB_URL should be set"),
            proc_dir: std::env::var("COLLECTOR_PROC_DIR").unwrap_or("/proc".to_string()),
            hostname_path: std::env::var("COLLECTOR_HOSTNAME_PATH")
                .unwrap_or("/etc/hostname".to_string()),
            server_port: std::env::var("COLLECTOR_SERVER_PORT")
                .unwrap_or("/etc/hostname".to_string())
                .parse()
                .expect("the environment variable COLLECTOR_SERVER_PORT should be set as a value between 1024 and 65535"),
        }
    }
}
// 1024 - 65535
pub struct Port(u16);

impl Deref for Port {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for Port {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(Error::MissingConfigValue);
        }

        let n: u16 = s
            .parse()
            .map_err(|_| Error::BadConfigValue(s.to_string()))?;

        if n < 1024 {
            return Err(Error::BadConfigValue(s.to_string()));
        }

        Ok(Self(n))
    }
}

impl Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
