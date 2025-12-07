//! TaskRun Control Plane Server

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

mod config;
mod crypto;
mod http;
mod metrics;
mod scheduler;
mod service;
mod state;

use config::Config;
use crypto::CertificateAuthority;
use service::{RunServiceImpl, TaskServiceImpl, WorkerServiceImpl};
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = Config::default();
    let grpc_addr: SocketAddr = config.bind_addr.parse()?;
    let http_addr: SocketAddr = config.http_bind_addr.parse()?;

    // Load TLS certificates for gRPC
    let cert = std::fs::read(&config.tls_cert_path).map_err(|e| {
        format!(
            "Failed to read TLS certificate from '{}': {}. Run scripts/gen-dev-certs.sh first.",
            config.tls_cert_path, e
        )
    })?;
    let key = std::fs::read(&config.tls_key_path).map_err(|e| {
        format!(
            "Failed to read TLS key from '{}': {}. Run scripts/gen-dev-certs.sh first.",
            config.tls_key_path, e
        )
    })?;

    let identity = Identity::from_pem(cert, key);

    // Load CA certificate for client verification (mTLS)
    let ca_cert_for_mtls = std::fs::read(&config.ca_cert_path).map_err(|e| {
        format!(
            "Failed to read CA certificate from '{}': {}. Run scripts/gen-dev-certs.sh first.",
            config.ca_cert_path, e
        )
    })?;

    // Configure TLS with mTLS (require client certificates signed by our CA)
    let tls_config = ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(Certificate::from_pem(ca_cert_for_mtls));

    info!("mTLS enabled - workers must present valid certificates");

    // Load CA for certificate signing (optional - enrollment won't work without it)
    let ca = match CertificateAuthority::from_files(
        &config.ca_cert_path,
        &config.ca_key_path,
        config.worker_cert_validity_days,
    ) {
        Ok(ca) => {
            info!(
                ca_cert = %config.ca_cert_path,
                validity_days = config.worker_cert_validity_days,
                "Certificate Authority loaded"
            );
            Some(ca)
        }
        Err(e) => {
            warn!(
                error = %e,
                "Failed to load CA - enrollment endpoint will not work"
            );
            None
        }
    };

    // Create shared state
    let state = match ca {
        Some(ca) => AppState::with_ca(ca),
        None => AppState::new(),
    };

    info!(grpc_addr = %grpc_addr, http_addr = %http_addr, "Starting TaskRun control plane");

    // Create gRPC services
    let run_service = RunServiceImpl::new(state.clone()).into_server();
    let task_service = TaskServiceImpl::new(state.clone()).into_server();
    let worker_service = WorkerServiceImpl::new(state.clone()).into_server();

    // Create HTTP router
    let http_router = http::create_router(state);

    // Start gRPC server
    let grpc_server = Server::builder()
        .tls_config(tls_config)?
        .add_service(run_service)
        .add_service(task_service)
        .add_service(worker_service)
        .serve(grpc_addr);

    // Start HTTP server
    let http_listener = TcpListener::bind(http_addr).await?;
    let http_server = axum::serve(http_listener, http_router);

    info!("gRPC server listening on {} (TLS)", grpc_addr);
    info!("HTTP server listening on {} (enrollment)", http_addr);

    // Run both servers concurrently
    tokio::select! {
        result = grpc_server => {
            if let Err(e) = result {
                tracing::error!(error = %e, "gRPC server error");
            }
        }
        result = http_server => {
            if let Err(e) = result {
                tracing::error!(error = %e, "HTTP server error");
            }
        }
    }

    Ok(())
}
