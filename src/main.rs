use axum::{
    Json, Router,
    http::{HeaderValue, Method},
    routing::get,
};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use vps_back::{
    config::Config, data_response, github_stats, middlewares, static_files::static_files_service,
};

#[tokio::main]
async fn main() {
    // Load .env file
    dotenvy::dotenv().ok();

    // Load configuration - will exit if required env vars are missing
    let config = Config::from_env().unwrap_or_else(|e| {
        eprintln!("Configuration error: {e}");
        std::process::exit(1);
    });

    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::new(&config.rust_log);

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Configure CORS
    let allowed_origins = config
        .allowed_origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .expect("Invalid origin in ALLOWED_ORIGINS")
        })
        .collect::<Vec<_>>();

    let mut cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE])
        .allow_credentials(true);

    // Add each allowed origin to the CORS configuration
    for origin in allowed_origins {
        cors = cors.allow_origin(origin);
    }

    // Build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .nest_service("/static", static_files_service(&config.static_dir))
        .layer(cors)
        .layer(middlewares::tracing::create_tracing_layer())
        .layer(axum::Extension(config.static_dir.clone()));

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    tracing::info!("listening on {}", listener.local_addr().unwrap());

    if let Some(token) = config.github_token.clone() {
        let static_dir = config.static_dir.clone();
        tokio::spawn(async move {
            github_stats::run_periodic_update(token, static_dir).await;
        });
    } else {
        tracing::warn!("GITHUB_TOKEN not set: GitHub stats will not be generated");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    tracing::info!("server stopped");
}

/// Waits for SIGTERM (sent by `docker stop`) or Ctrl+C.
///
/// In a container, the server is PID 1. Linux gives PID 1 no default action
/// for SIGTERM: without this handler, the signal does nothing, and Docker
/// kills the server after its stop timeout (10 seconds).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received: the server finishes the open requests, then stops");
}

/// Handles GET requests to the root path ("/").
/// Serves as a simple health check endpoint.
async fn root() -> Json<serde_json::Value> {
    tracing::info!("GET `/` endpoint called");

    data_response(json!({
        "message": "Hello, I'm Tom Planche!"
    }))
}
