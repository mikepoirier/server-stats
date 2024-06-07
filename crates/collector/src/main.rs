use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use metrics_core::{
    metrics_service_server::{MetricsService, MetricsServiceServer},
    registration_service_client::RegistrationServiceClient,
    Memory, MetricsRequest, MetricsResponse, RegistrationRequest,
};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, level_filters::LevelFilter, warn};
use tracing_subscriber::EnvFilter;

mod config;

pub type Result<T> = core::result::Result<T, Box<dyn std::error::Error + 'static>>;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::config().await;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    println!("Collector started");
    let grpc_addr = format!("127.0.0.1:{}", config.server_port).parse().unwrap();

    let handle = tokio::spawn(async move {
        info!("Starting server at {grpc_addr}");
        Server::builder()
            .add_service(MetricsServiceServer::new(CollectorMetricService::new(
                &config.hostname_path,
                &config.proc_dir,
            )))
            .serve(grpc_addr)
            .await
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut connection_attempts = 0;
    let mut registration_client = loop {
        connection_attempts += 1;
        info!("Connection attempt: {connection_attempts}");
        match RegistrationServiceClient::connect(config.web_url.as_str()).await {
            Ok(client) => {
                info!("Client connected");
                break client;
            }
            Err(e) => {
                warn!("Connection error: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        };
        if connection_attempts >= 5 {
            error!("Exhausted connection retry attempts. Exiting.");
            panic!("Exhausted connection retry attempts. Exiting.");
        }
    };

    let mut registration_errors = 0;
    while let Err(e) = registration_client
        .register(RegistrationRequest {
            port: config.server_port.to_string(),
        })
        .await
    {
        warn!("Registration Error: {e}");
        tokio::time::sleep(Duration::from_secs(1)).await;
        registration_errors += 1;

        if registration_errors >= 5 {
            panic!("Registration Fatal Error. Exhausted Retries.")
        }
    }

    handle.await??;

    Ok(())
}

struct CollectorMetricService {
    hostname_file: String,
    proc_dir: String,
}

impl CollectorMetricService {
    pub fn new(host: impl Into<String>, proc_dir: impl Into<String>) -> Self {
        Self {
            hostname_file: host.into(),
            proc_dir: proc_dir.into(),
        }
    }
}

#[tonic::async_trait]
impl MetricsService for CollectorMetricService {
    async fn request_metrics(
        &self,
        _request: Request<MetricsRequest>,
    ) -> core::result::Result<Response<MetricsResponse>, Status> {
        Ok(Response::new(MetricsResponse {
            host: hostname(&self.hostname_file).await.map_err(|e| {
                warn!("Error: {e}");
                Status::internal("Could not get hostname")
            })?,
            cpu_usage: 0.0,
            memory: memory_usage(&self.proc_dir)
                .await
                .map_err(|e| {
                    warn!("Error: {e}");
                    Status::internal("Could not get memory")
                })
                .ok(),
            net_usage: 0,
        }))
    }
}

async fn hostname(etc_hostname_path: impl AsRef<Path>) -> Result<String> {
    let file = tokio::fs::read_to_string(etc_hostname_path).await?;
    Ok(file.trim().to_string())
}

async fn memory_usage(proc_dir: impl Into<PathBuf>) -> Result<Memory> {
    let mut meminfo_path: PathBuf = proc_dir.into();
    meminfo_path.push("meminfo");
    let file = tokio::fs::read_to_string(meminfo_path).await?;

    let mut mem_total = 0;
    let mut mem_free = 0;
    let mut mem_available = 0;
    let mut buffers = 0;
    let mut cached = 0;

    for line in file.lines() {
        if line.starts_with("MemTotal") {
            let mut parts = line.split_whitespace();
            mem_total = parts.nth(1).unwrap().parse::<u64>().unwrap() * 1000;
        }
        if line.starts_with("MemAvailable") {
            let mut parts = line.split_whitespace();
            mem_available = parts.nth(1).unwrap().parse::<u64>().unwrap() * 1000;
        }
        if line.starts_with("MemFree") {
            let mut parts = line.split_whitespace();
            mem_free = parts.nth(1).unwrap().parse::<u64>().unwrap() * 1000;
        }
        if line.starts_with("Buffers") {
            let mut parts = line.split_whitespace();
            buffers = parts.nth(1).unwrap().parse::<u64>().unwrap() * 1000;
        }
        if line.starts_with("Cached") {
            let mut parts = line.split_whitespace();
            cached = parts.nth(1).unwrap().parse::<u64>().unwrap() * 1000;
        }
    }

    Ok(Memory {
        mem_total,
        mem_free,
        mem_available,
        buffers,
        cached,
    })
}
