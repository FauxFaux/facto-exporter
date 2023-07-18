mod bulk_unit;
mod by_unit;
mod long_time;

use std::future::Future;
use std::sync::Arc;
use std::{fs, io};

use anyhow::Result;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use bunyarrs::{vars, vars_dbg, Bunyarr};
use serde_json::{json, Value};

use facto_exporter::{unpack_observation, Observation};

pub struct Data {
    inner: Vec<Observation>,
}

pub struct AppState {
    data: Arc<tokio::sync::RwLock<Data>>,
    logger: Bunyarr,
}

const KNOWN_STATUSES: [(u32, &'static str); 12] = [
    (1, "working"),
    (2, "normal"),
    (37, "no_power"),
    (12, "low_power"),
    (36, "no_fuel"),
    (38, "disabled_by_control_behaviour"),
    (41, "disabled_by_script"),
    (43, "marked_for_deconstruction"),
    (15, "no_recipe"),
    (20, "fluid_ingredient_shortage"),
    (22, "full_output"),
    (21, "item_ingredient_shortage"),
];

#[tokio::main]
async fn main() -> Result<()> {
    let logger = bunyarrs::Bunyarr::with_name("serve");

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
        // when the exporter has crashed during startup? boo
        if path.metadata()?.len() == 0 {
            continue;
        }
        let path = path.path();
        logger.info(vars! { path }, "loading observation");
        let mut archiv =
            archiv::ExpandOptions::default().stream(io::BufReader::new(fs::File::open(path)?))?;

        loop {
            let item = match archiv.next_item() {
                Err(err) => {
                    logger.warn(
                        vars_dbg! { err },
                        "failed to read item, assuming live archive",
                    );
                    break;
                }
                Ok(None) => break,
                Ok(Some(item)) => item,
            };
            data.inner.push(unpack_observation(item)?);
        }
    }

    use axum::routing::*;
    let app = Router::new()
        .route("/", get(|| async { env!("CARGO_PKG_NAME") }))
        .route("/healthcheck", get(|| async { "ok" }))
        .route("/metrics/raw", get(bulk_unit::metrics_raw))
        .route("/exp/store", post(store))
        .route("/api/query", get(by_unit::query))
        .route("/api/last", get(by_unit::last))
        .route("/api/long", get(long_time::long))
        .route("/api/bulk-status", get(bulk_unit::bulk_status))
        .with_state(Arc::new(AppState {
            data: Arc::new(tokio::sync::RwLock::new(data)),
            logger: Bunyarr::with_name("handler"),
        }));

    let port = 9429;
    logger.info(vars! { port }, "starting server");
    axum::Server::bind(&([127, 0, 0, 1], port).into())
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[axum::debug_handler]
async fn store(State(state): State<Arc<AppState>>, buf: Bytes) -> StatusCode {
    // TODO: less copying / unbounded memory usage?
    let buf = buf.to_vec();
    let observation = match unpack_observation(io::Cursor::new(buf)) {
        Ok(observation) => observation,
        Err(err) => {
            eprintln!("error parsing observation: {}", err);
            return StatusCode::BAD_REQUEST;
        }
    };

    let mut data = state.data.write().await;
    data.inner.push(observation);
    StatusCode::ACCEPTED
}

async fn okay_or_500<F: Future<Output = Result<Value>>>(
    logger: &Bunyarr,
    func: impl FnOnce() -> F,
) -> (StatusCode, Json<Value>) {
    match func().await {
        Ok(resp) => (StatusCode::OK, Json(resp)),
        Err(err) => {
            logger.error(vars_dbg!(err), "error handling request");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error "})),
            )
        }
    }
}
