use axum::{
    extract::State,
    middleware,
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use clap::Parser;
use std::{net::IpAddr, sync::Arc};
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

mod auth;
mod grpc;
mod http_routes;

pub struct AppState {
    pub store: cc_store::db::Store,
    pub tx: broadcast::Sender<String>,
    pub auth_token: Option<String>,
    pub readonly: bool,
}

#[derive(Parser)]
#[command(name = "cascade-server", about = "Cascade — config center server")]
struct Args {
    #[arg(long, default_value = "7070")]
    port: u16,
    /// Bind address. Loopback by default; non-loopback REQUIRES --token.
    #[arg(long, default_value = "127.0.0.1")]
    listen: String,
    /// Bearer token for API auth. Required when --listen is not loopback.
    #[arg(long)]
    token: Option<String>,
    /// Reject all non-GET requests with 403.
    #[arg(long)]
    readonly: bool,
    /// gRPC listen port. Defaults to --port + 1.
    #[arg(long)]
    grpc_port: Option<u16>,
    /// Disable the gRPC server.
    #[arg(long)]
    no_grpc: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let listen_ip: IpAddr = args.listen.parse().map_err(|_| {
        anyhow::anyhow!("invalid --listen address: {}", args.listen)
    })?;
    if !listen_ip.is_loopback() && args.token.is_none() {
        anyhow::bail!(
            "refusing to listen on non-loopback {} without --token (would expose unauthenticated API)",
            args.listen
        );
    }

    let store = cc_store::db::Store::open_default()?;

    if let Some(tok) = &args.token {
        // Keep existing permissions: a scoped (read-only) share token must not
        // be silently upgraded to admin by a server restart.
        cc_store::token_repo::TokenRepo::new(&store).ensure(tok, "admin")?;
        tracing::info!("API token auth enabled");
    } else {
        tracing::warn!("no --token set: API is unauthenticated (loopback only)");
    }

    let (tx, _) = broadcast::channel(100);
    let state = Arc::new(AppState {
        store,
        tx,
        auth_token: args.token.clone(),
        readonly: args.readonly,
    });

    let app = Router::new()
        .route("/", get(status_page))
        .route(
            "/api/configs",
            get(http_routes::config::list).post(http_routes::config::create),
        )
        .route(
            "/api/configs/:id",
            get(http_routes::config::get)
                .put(http_routes::config::update)
                .delete(http_routes::config::delete),
        )
        .route(
            "/api/configs/:id/history",
            get(http_routes::config::history),
        )
        .route(
            "/api/projects",
            get(http_routes::project::list).post(http_routes::project::create),
        )
        .route(
            "/api/projects/:id",
            get(http_routes::project::get).delete(http_routes::project::delete),
        )
        .route(
            "/api/projects/:id/configs",
            get(http_routes::project::list_configs).post(http_routes::project::add_config),
        )
        .route(
            "/api/projects/:id/resolved",
            get(http_routes::project::resolved),
        )
        .route(
            "/api/envs",
            get(http_routes::env::list).post(http_routes::env::create),
        )
        .route(
            "/api/envs/:id",
            get(http_routes::env::get).delete(http_routes::env::delete),
        )
        .route("/api/sse/configs", get(sse_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let addr = format!("{}:{}", args.listen, args.port);
    tracing::info!(%addr, readonly = args.readonly, "cascade HTTP listening");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let http = axum::serve(listener, app);

    if args.no_grpc {
        http.await?;
        return Ok(());
    }

    let grpc_port = args.grpc_port.unwrap_or_else(|| args.port.saturating_add(1));
    let grpc_addr = format!("{}:{}", args.listen, grpc_port);
    let svc = grpc::GrpcService::new(state.clone());
    let grpc = tonic::transport::Server::builder()
        .add_service(
            grpc::proto::config_center_server::ConfigCenterServer::with_interceptor(
                svc,
                grpc::auth_interceptor(state),
            ),
        )
        .serve(grpc_addr.parse().map_err(|e| anyhow::anyhow!("bad grpc addr: {}", e))?);
    tracing::info!(addr = %grpc_addr, "cascade gRPC listening");

    tokio::try_join!(
        async { http.await.map_err(|e| anyhow::anyhow!("http: {}", e)) },
        async { grpc.await.map_err(|e| anyhow::anyhow!("grpc: {}", e)) },
    )?;
    Ok(())
}

/// Minimal status dashboard (also the target of `cc serve --open`).
async fn status_page(State(state): State<Arc<AppState>>) -> axum::response::Html<String> {
    let n_configs = cc_store::config_repo::ConfigRepo::new(&state.store)
        .list(None)
        .map(|v| v.len())
        .unwrap_or(0);
    let n_projects = cc_store::project_repo::ProjectRepo::new(&state.store)
        .list()
        .map(|v| v.len())
        .unwrap_or(0);
    let n_envs = cc_store::env_repo::EnvRepo::new(&state.store)
        .list()
        .map(|v| v.len())
        .unwrap_or(0);
    let auth = if state.auth_token.is_some() {
        "token"
    } else {
        "none (loopback only)"
    };
    axum::response::Html(format!(
        "<!DOCTYPE html><html><head><meta charset=utf-8><title>Cascade</title></head>\
        <body style=\"font-family:sans-serif;max-width:640px;margin:4rem auto\">\
        <h1>Cascade</h1>\
        <ul><li>configs: {}</li><li>projects: {}</li><li>environments: {}</li>\
        <li>auth: {}</li><li>mode: {}</li></ul>\
        <p>API: <code>GET /api/configs</code> · events: <code>GET /api/sse/configs</code></p>\
        </body></html>",
        n_configs,
        n_projects,
        n_envs,
        auth,
        if state.readonly { "readonly" } else { "read-write" },
    ))
}

async fn sse_handler(    State(state): State<Arc<AppState>>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.tx.subscribe();

    let stream = futures::stream::unfold(rx, |mut rx| async {
        match rx.recv().await {
            Ok(msg) => Some((Ok(Event::default().data(msg)), rx)),
            Err(_) => None,
        }
    });

    Sse::new(stream)
}
