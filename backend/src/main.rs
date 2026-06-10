use std::env;
use std::time::Duration;

use axum::http::{HeaderValue, Method};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

mod groq;
mod models;
mod music;
mod routes;
mod usda;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http: reqwest::Client,
    pub groq_key: String,
    pub usda_key: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "life_api=info,tower_http=info".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let groq_key = env::var("GROQ_API_KEY")?;
    let usda_key = env::var("USDA_API_KEY").unwrap_or_else(|_| "DEMO_KEY".into());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;

    let allowed_origins = [
        "https://tejasprabhune.github.io",
        "http://localhost:5173",
        "http://127.0.0.1:5173",
    ]
    .map(|o| o.parse::<HeaderValue>().unwrap());

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let state = AppState { pool, http, groq_key, usda_key };

    let app = Router::new()
        .route("/health", get(routes::health))
        .route("/api/logs", get(routes::list_logs).post(routes::create_log))
        .route(
            "/api/logs/{id}",
            get(routes::get_log)
                .patch(routes::update_log)
                .delete(routes::delete_log),
        )
        .route("/api/albums", get(music::list_albums))
        .route("/api/albums/{id}/rank", post(music::rank_album))
        .route("/api/songs", get(music::list_songs))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "8080".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}
