use anyhow::{ensure, Result};
use axum::body::Bytes;
use axum::extract::State;
use reqwest::StatusCode;
use std::io::Read;
use std::sync::Arc;
use std::{fs, io};

use facto_exporter::{unpack_observation, Observation};

struct Data {
    inner: Vec<Observation>,
}

#[derive(Clone)]
struct AppState {
    data: Arc<tokio::sync::Mutex<Data>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut data = Data {
        inner: Vec::with_capacity(256),
    };

    for path in fs::read_dir(".")? {
        let path = path?;
        if !path
            .file_name()
            .to_string_lossy()
            .ends_with(".facto-cp.archiv")
        {
            continue;
        }
        let mut archiv = archiv::ExpandOptions::default()
            .stream(io::BufReader::new(fs::File::open(path.file_name())?))?;

        let mut bytes = Vec::with_capacity(10 * 1024);
        while let Some(mut item) = archiv.next_item()? {
            item.read_to_end(&mut bytes)?;
            data.inner
                .push(unpack_observation(io::Cursor::new(&bytes))?);
            bytes.clear();
        }
    }

    use axum::routing::*;
    let app = axum::Router::new()
        .route("/", get(|| async { env!("CARGO_PKG_NAME") }))
        .route("/healthcheck", get(|| async { "ok" }))
        .route("/metrics/raw", get(metrics_raw))
        .route("/exp/store", post(store))
        .with_state(AppState {
            data: Arc::new(tokio::sync::Mutex::new(data)),
        });

    axum::Server::bind(&([127, 0, 0, 1], 9429).into())
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[axum::debug_handler]
async fn store(State(state): State<AppState>, buf: Bytes) -> StatusCode {
    // TODO: less copying / unbounded memory usage?
    let buf = buf.to_vec();
    let observation = match unpack_observation(io::Cursor::new(buf)) {
        Ok(observation) => observation,
        Err(err) => {
            eprintln!("error parsing observation: {}", err);
            return StatusCode::BAD_REQUEST;
        }
    };

    let mut data = state.data.lock().await;
    data.inner.push(observation);
    StatusCode::ACCEPTED
}

#[axum::debug_handler]
async fn metrics_raw(State(state): State<AppState>) -> String {
    let data = state.data.lock().await;
    let data = match data.inner.last() {
        Some(data) => data,
        None => return String::new(),
    };

    let mut s = String::with_capacity(data.inner.len() * 50);
    for crafting in &data.inner {
        s.push_str(&format!(
            "facto_products_complete{{unit=\"{}\"}} {}\n",
            crafting.unit_number, crafting.products_complete,
        ));
    }

    s
}
