use anyhow::Result;
use std::sync::Arc;

struct Data {}

struct AppState {
    data: Arc<tokio::sync::Mutex<Data>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    use axum::routing::*;
    let app = axum::Router::new()
        .route("/", get(|| async { env!("CARGO_PKG_NAME") }))
        .route("/healthcheck", get(|| async { r#"{ "status": "ok" }"# }))
        .route("/exp/store", post(store))
        .with_state(Arc::new(AppState {
            data: Arc::new(tokio::sync::Mutex::new(Data {})),
        }));
    axum::Server::bind(&([127, 0, 0, 1], 9429).into())
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn store() -> &'static str {
    "store"
}
