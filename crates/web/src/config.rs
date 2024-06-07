use tokio::sync::OnceCell;

pub async fn config() -> &'static Config {
    static INSTANCE: OnceCell<Config> = OnceCell::const_new();
    INSTANCE.get_or_init(|| async { Config::new() }).await
}
pub struct Config {
    pub grpc: GRPCConfig,
    pub web: WebConfig,
}

impl Config {
    fn new() -> Self {
        Self {
            grpc: GRPCConfig {
                host: env_or_default("WEB_GRPC_HOST", "127.0.0.1"),
                port: env_or_default("WEB_GRPC_PORT", "3001"),
            },
            web: WebConfig {
                host: env_or_default("WEB_WEB_HOST", "127.0.0.1"),
                port: env_or_default("WEB_WEB_PORT", "3000"),
            },
        }
    }
}

pub struct GRPCConfig {
    pub host: String,
    pub port: String,
}

pub struct WebConfig {
    pub host: String,
    pub port: String,
}

fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or(default.to_string())
}
