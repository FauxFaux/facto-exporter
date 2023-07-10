use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::{fs, io};

use anyhow::{anyhow, ensure, Context, Result};
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
        .route("/api/last", get(last))
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

#[axum::debug_handler]
async fn metrics_raw(State(state): State<Arc<AppState>>) -> String {
    let data = state.data.read().await;
    let data = match data.inner.last() {
        Some(data) => data,
        None => return String::new(),
    };

    let status_lookup = KNOWN_STATUSES
        .iter()
        .copied()
        .collect::<std::collections::HashMap<_, _>>();

    let mut s = String::with_capacity(data.inner.len() * 50);
    for crafting in &data.inner {
        s.push_str(&format!(
            "facto_products_complete{{unit=\"{}\"}} {}\n",
            crafting.unit_number, crafting.products_complete,
        ));
        s.push_str(&format!(
            "# {}\nfacto_status{{unit=\"{}\"}} {}\n",
            status_lookup.get(&crafting.status).unwrap_or(&"unknown"),
            crafting.unit_number,
            crafting.status,
        ));
    }

    s
}

fn split_units(logger: &Bunyarr, units: &str) -> Option<Vec<u32>> {
    match units
        .split(',')
        .map(|s| -> Result<u32> { Ok(s.parse::<u32>().with_context(|| anyhow!("{s:?}"))?) })
        .collect::<Result<Vec<u32>>>()
    {
        Ok(mut units) => {
            units.sort_unstable();
            Some(units)
        }
        Err(err) => {
            logger.warn(vars_dbg! { err }, "failed to parse units");
            None
        }
    }
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
    let units = match split_units(&state.logger, &query.units) {
        Some(units) => units,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid units" })),
            );
        }
    };

    let end = query
        .end
        .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp());

    let steps = query.steps.unwrap_or(30);
    let gap = query.gap.unwrap_or(60);

    okay_or_500(&state.logger, || async {
        let data = state.data.read().await;

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

        let mut unit_data = Vec::with_capacity(units.len());
        for _ in &units {
            unit_data.push(Vec::with_capacity(all_obses.len()));
        }

        for obs in obses {
            assert_eq!(unit_data.len(), units.len());
            for (by_unit, unit) in unit_data.iter_mut().zip(units.iter()) {
                if let Ok(found) = obs
                    .inner
                    .binary_search_by_key(&unit, |crafting| &crafting.unit_number)
                {
                    let crafting = &obs.inner[found];
                    by_unit.push(Some((crafting.products_complete, crafting.status)));
                } else {
                    by_unit.push(None);
                }
            }
        }

        let statuses = unit_data
            .iter()
            .map(|unit_data| {
                unit_data
                    .iter()
                    .map(|opt| opt.map(|(_, s)| s))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let deltas = unit_data
            .into_iter()
            .map(|deltas| {
                deltas
                    .iter()
                    .zip(deltas.iter().skip(1))
                    .map(|(a, b)| match (a, b) {
                        (Some((a, _)), Some((b, _))) => b.checked_sub(*a),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        Ok(json!({ "units": units, "deltas": deltas, "statuses": statuses, "times": times }))
    })
    .await
}

#[derive(serde::Deserialize)]
struct LastQuery {
    // Vec<u32> csv
    units: String,
}

#[derive(serde::Serialize, Default)]
#[serde(rename_all="camelCase")]
struct UnitData {
    produced_change: Option<i64>,
    last_status: Option<u32>,
    last_status_change: Option<i64>,
    previous_status: Option<u32>,
}

impl UnitData {
    fn all_complete(&self) -> bool {
        self.produced_change.is_some()
            && self.last_status.is_some()
            && self.previous_status.is_some()
            && self.last_status_change.is_some()
    }
}

#[axum::debug_handler]
async fn last(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LastQuery>,
) -> impl IntoResponse {
    let units = match split_units(&state.logger, &query.units) {
        Some(units) => units,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid units" })),
            );
        }
    };

    okay_or_500(&state.logger, || async {
        let data = state.data.read().await;
        ensure!(!data.inner.is_empty(), "no data");

        let mut changes = HashMap::with_capacity(units.len());

        for unit in &units {
            let mut unit_data = UnitData::default();
            let mut produced_previous = None;
            let mut status_previous = None;
            for obs in data.inner.iter().rev() {
                let found = match obs
                    .inner
                    .binary_search_by_key(&unit, |crafting| &crafting.unit_number)
                {
                    Ok(found) => found,
                    Err(_) => continue,
                };
                let found = &obs.inner[found];
                if unit_data.produced_change.is_none() {
                    if produced_previous.is_some()
                        && produced_previous != Some(found.products_complete)
                    {
                        unit_data.produced_change = Some(obs.time.unix_timestamp());
                    }
                    produced_previous = Some(found.products_complete);
                }
                if unit_data.last_status.is_none() {
                    unit_data.last_status = Some(found.status);
                }
                if unit_data.last_status_change.is_none() {
                    if status_previous.is_some() && status_previous != Some(found.status) {
                        unit_data.last_status_change = Some(obs.time.unix_timestamp());
                        unit_data.previous_status = Some(found.status);
                    }
                    status_previous = Some(found.status);
                }

                if unit_data.all_complete() {
                    break;
                }
            }
            changes.insert(unit, unit_data);
        }

        Ok(json!({ "changes": changes }))
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
