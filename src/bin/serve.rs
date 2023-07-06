use anyhow::{anyhow, ensure, Result};
use axum::body::Bytes;
use axum::extract::State;
use bytes::Buf;
use reqwest::StatusCode;
use std::io::Read;
use std::sync::Arc;
use std::{fs, io};
use time::OffsetDateTime;

use facto_exporter::{CraftingLite, Observation};

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

        while let Some(mut item) = archiv.next_item()? {
            let mut bytes = Vec::with_capacity(10 * 1024);
            item.read_to_end(&mut bytes)?;
            data.inner.push(parse_observation(Bytes::from(bytes))?);
        }
    }

    use axum::routing::*;
    let app = axum::Router::new()
        .route("/", get(|| async { env!("CARGO_PKG_NAME") }))
        .route("/healthcheck", get(|| async { r#"{ "status": "ok" }"# }))
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
    let observation = match parse_observation(buf) {
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

fn parse_observation(mut buf: Bytes) -> Result<Observation> {
    ensure!(buf.remaining() >= 16, "lacks header");
    let records = buf.get_u64_le();
    let time = OffsetDateTime::from_unix_timestamp(buf.get_i64_le())?;
    ensure!(
        records < u32::MAX as u64,
        "implausible records number: {records}"
    );
    ensure!(
        buf.remaining()
            == usize::try_from(records)?
                .checked_mul(8)
                .ok_or_else(|| anyhow!("overflow in records number: {records}"))?,
        "invalid length: {} for {records}",
        buf.remaining()
    );

    let mut inner = Vec::with_capacity(records.min(4096) as usize);
    for _ in 0..records {
        inner.push(CraftingLite {
            unit_number: buf.get_u32_le(),
            products_complete: buf.get_u32_le(),
        });
    }

    assert_eq!(buf.remaining(), 0, "parser dumbness");

    Ok(Observation { time, inner })
}
