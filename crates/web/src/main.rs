use std::{error::Error, sync::Arc};

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use maud::{html, Markup, Render, DOCTYPE};
use metrics_core::{
    metrics_service_client::MetricsServiceClient, registration_service_server::*, MetricsRequest,
    MetricsResponse, RegistrationRequest, RegistrationResponse,
};
use tokio::{net::TcpListener, sync::Mutex};
use tonic::{
    transport::{Channel, Server},
    Request, Response, Status,
};
use tracing::{info, level_filters::LevelFilter, warn};
use tracing_subscriber::EnvFilter;

mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();
    info!("Web Server Starting");
    let config = config::config().await;

    let connectors = vec![];
    let connectors = Arc::new(Mutex::new(connectors));
    let service = MyRegistrationService::new(connectors.clone());
    let grpc_addr = format!("{}:{}", config.grpc.host, config.grpc.port)
        .parse()
        .unwrap();

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(RegistrationServiceServer::new(service))
            .serve(grpc_addr)
            .await
    });

    let app_state = AppState::new(connectors);

    let routes = Router::new()
        .route("/", get(root))
        .route("/metrics", get(metrics))
        .with_state(app_state);

    let listener = TcpListener::bind(format!("{}:{}", config.web.host, config.web.port)).await?;

    axum::serve(listener, routes).await?;

    handle.await??;

    Ok(())
}

struct MyRegistrationService {
    connectors: Arc<Mutex<Vec<MetricsServiceClient<Channel>>>>,
}

impl MyRegistrationService {
    pub fn new(connectors: Arc<Mutex<Vec<MetricsServiceClient<Channel>>>>) -> Self {
        Self { connectors }
    }
}

#[tonic::async_trait]
impl RegistrationService for MyRegistrationService {
    async fn register(
        &self,
        request: Request<RegistrationRequest>,
    ) -> Result<Response<RegistrationResponse>, Status> {
        let remote_addr = request.remote_addr().unwrap();
        let body = request.into_inner();
        let port = body.port;

        let connection = format!("http://{}:{}", remote_addr.ip(), port);
        info!("Trying to connect to {connection}");

        let client = MetricsServiceClient::connect(connection)
            .await
            .map_err(|e| {
                let source = e.source();
                warn!("Metrics Service Connection Error: {e} {source:?}");
                Status::internal("Could not connect to collector")
            })?;

        let mut connectors = self.connectors.lock().await;
        connectors.push(client);

        Ok(Response::new(RegistrationResponse {
            status: "OK".to_string(),
        }))
    }
}

#[derive(Clone)]
struct AppState {
    connectors: Arc<Mutex<Vec<MetricsServiceClient<Channel>>>>,
}

impl AppState {
    pub fn new(connectors: Arc<Mutex<Vec<MetricsServiceClient<Channel>>>>) -> Self {
        Self { connectors }
    }
}

async fn root() -> core::result::Result<impl IntoResponse, StatusCode> {
    Ok(Html(
        page(html! {
            h1 { "Metrics" }
            div hx-get="/metrics" hx-trigger="load" hx-swap="outerHTML" {}
        })
        .render()
        .into_string(),
    ))
}

async fn metrics(
    State(app_state): State<AppState>,
) -> core::result::Result<impl IntoResponse, StatusCode> {
    let mut clients = app_state.connectors.lock().await;
    let mut metrics = vec![];
    for client in clients.iter_mut() {
        let resp = client
            .request_metrics(MetricsRequest {})
            .await
            .map_err(|e| {
                warn!("{e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        metrics.push(resp.into_inner());
    }
    Ok(Html(
        html! {
            div hx-get="/metrics" hx-trigger="load delay:3s" hx-swap="outerHTML" {
                @for metric in metrics {
                    h2 { (metric.host) }
                    p { "Memory Total: " (mem_total(&metric)) }
                    p { "Memory Free: " (mem_free(&metric)) }
                    p { "Buffers: " (buffers(&metric)) }
                    p { "Cached: " (cached(&metric)) }
                    p { "Memory Available: " (mem_available(&metric)) }
                    p { "Used: " (format!("{:.2}", pct_used(&metric))) " " (used(&metric)) }
                }
            }
        }
        .render()
        .into_string(),
    ))
}

fn mem_total(metric: &MetricsResponse) -> u64 {
    metric
        .memory
        .as_ref()
        .map(|m| m.mem_total)
        .unwrap_or_default()
}

fn mem_free(metric: &MetricsResponse) -> u64 {
    metric
        .memory
        .as_ref()
        .map(|m| m.mem_free)
        .unwrap_or_default()
}

fn mem_available(metric: &MetricsResponse) -> u64 {
    metric
        .memory
        .as_ref()
        .map(|m| m.mem_available)
        .unwrap_or_default()
}

fn buffers(metric: &MetricsResponse) -> u64 {
    metric
        .memory
        .as_ref()
        .map(|m| m.buffers)
        .unwrap_or_default()
}

fn cached(metric: &MetricsResponse) -> u64 {
    metric.memory.as_ref().map(|m| m.cached).unwrap_or_default()
}

fn used(metric: &MetricsResponse) -> u64 {
    let total = mem_total(metric);
    let free = mem_free(metric);
    let buffers = buffers(metric);
    let cached = cached(metric);
    total - free - buffers - cached
}

fn pct_used(metric: &MetricsResponse) -> f64 {
    let total = mem_total(metric);
    let used = used(metric);
    (used as f64) / (total as f64)
}

fn page(content: impl Render) -> Markup {
    let head = html! {
        head {
            title { "Metrics" }
            script src="https://unpkg.com/htmx.org@1.9.12"
                integrity="sha384-ujb1lZYygJmzgSwoxRggbCHcjc0rB2XoQrxeTUQyRjrOnlCoYta87iKBWq3EsdM2"
                crossorigin="anonymous" {}
        }
    };
    html! {
        (DOCTYPE)
        html {
            (head)
            body { (content) }
        }
    }
}
