use std::future::Future;
use std::sync::Arc;
use std::{fs, io};

use anyhow::{ensure, Result};
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use bunyarrs::{vars, vars_dbg, Bunyarr};
use serde_json::{json, Value};
use time::OffsetDateTime;

use facto_exporter::{unpack_observation, Observation};

struct Data {
    inner: Vec<Observation>,
}

struct AppState {
    data: Arc<tokio::sync::Mutex<Data>>,
    logger: Bunyarr,
}

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
        .route("/metrics/raw", get(metrics_raw))
        .route("/exp/store", post(store))
        .route("/api/query", get(query))
        .with_state(Arc::new(AppState {
            data: Arc::new(tokio::sync::Mutex::new(data)),
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

    let mut data = state.data.lock().await;
    data.inner.push(observation);
    StatusCode::ACCEPTED
}

#[axum::debug_handler]
async fn metrics_raw(State(state): State<Arc<AppState>>) -> String {
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

#[derive(serde::Deserialize)]
struct QueryQuery {
    // number of observations to return, default 30
    steps: Option<u32>,
    // number of seconds between each observation, default 60
    gap: Option<u32>,
    // unix seconds, default now()
    end: Option<i64>,
    // Vec<u32> csv
    units: String,
}

#[axum::debug_handler]
async fn query(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryQuery>,
) -> impl IntoResponse {
    let mut units = match query
        .units
        .split(',')
        .map(|s| -> Result<u32> { Ok(s.parse::<u32>()?) })
        .collect::<Result<Vec<u32>>>()
    {
        Ok(units) => units,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid units" })),
            )
        }
    };

    units.sort_unstable();

    let end = query
        .end
        .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp());

    let steps = query.steps.unwrap_or(30);
    let gap = query.gap.unwrap_or(60);

    okay_or_500(&state.logger, || async {
        let data = state.data.lock().await;

        ensure!(!data.inner.is_empty(), "no data");

        let all_obses = data
            .inner
            .iter()
            .map(|obs| obs.time.unix_timestamp())
            .collect::<Vec<_>>();

        // find the nearest ones for our chosen steps
        let mut obses = Vec::with_capacity(steps as usize);
        for step in 0..steps {
            let target = end - (step * gap) as i64;
            let best = match all_obses.binary_search(&target) {
                Ok(a) => a,
                Err(a) => a,
            };

            obses.push(
                data.inner
                    .get(best)
                    .unwrap_or_else(|| data.inner.last().expect("non-empty observations")),
            );
        }

        obses.reverse();

        let times = obses
            .iter()
            .map(|obs| obs.time.unix_timestamp())
            .collect::<Vec<_>>();

        let mut deltas = Vec::with_capacity(units.len());
        for _ in &units {
            deltas.push(Vec::with_capacity(all_obses.len()));
        }

        for obs in obses {
            assert_eq!(deltas.len(), units.len());
            for (by_unit, unit) in deltas.iter_mut().zip(units.iter()) {
                if let Ok(found) = obs
                    .inner
                    .binary_search_by_key(&unit, |crafting| &crafting.unit_number)
                {
                    let crafting = &obs.inner[found];
                    by_unit.push(Some(crafting.products_complete));
                } else {
                    by_unit.push(None);
                }
            }
        }

        let deltas = deltas
            .into_iter()
            .map(|deltas| {
                deltas
                    .iter()
                    .zip(deltas.iter().skip(1))
                    .map(|(a, b)| match (a, b) {
                        (Some(a), Some(b)) => b.checked_sub(*a),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        Ok(json!({ "units": units, "deltas": deltas, "times": times }))
    })
    .await
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
