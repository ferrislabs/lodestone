use axum::Router;
use axum_server::Handle;
use std::net::{SocketAddr, ToSocketAddrs};
use tracing::info;

use crate::errors::ServerError;

pub mod errors;

pub async fn get_addr(host: &str, port: u16) -> Result<SocketAddr, ServerError> {
    let addrs = format!("{}:{}", host, port)
        .to_socket_addrs()
        .map_err(|e| ServerError::SocketResolveError(e.to_string()))?
        .collect::<Vec<SocketAddr>>();

    let socket = match addrs.first() {
        Some(addr) => *addr,
        None => {
            return Err(ServerError::SocketNotFound(
                "No socket adresses found".into(),
            ));
        }
    };

    Ok(socket)
}

pub async fn run_server(
    addr: SocketAddr,
    router: Router,
    handle: Handle<SocketAddr>,
) -> Result<(), ServerError> {
    info!("listening on {addr}");

    axum_server::bind(addr)
        .handle(handle)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|e| ServerError::Internal {
            message: e.to_string(),
        })
}

pub async fn shutdown_signal() {
    let interrupt = tokio::signal::ctrl_c();
    let mut terminate =
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(%error, "cannot listen for SIGTERM, only Ctrl-C stops the process");
                let _ = interrupt.await;
                return;
            }
        };
    tokio::select! {
        _ = interrupt => info!("received Ctrl-C"),
        _ = terminate.recv() => info!("received SIGTERM"),
    }
}
