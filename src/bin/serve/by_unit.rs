use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, ensure, Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use bunyarrs::{vars_dbg, Bunyarr};
use serde_json::json;
use time::OffsetDateTime;

use crate::{okay_or_500, AppState, Data};

pub fn split_units(logger: &Bunyarr, units: &str) -> Option<Vec<u32>> {
    match units
        .split(',')
        .map(|s| -> Result<u32> { Ok(s.parse::<u32>().with_context(|| anyhow!("{s:?}"))?) })
        .collect::<Result<Vec<u32>>>()
    {
        Ok(mut units) => {
            units.sort_unstable();
            units.dedup();
            Some(units)
        }
        Err(err) => {
            logger.warn(vars_dbg! { err }, "failed to parse units");
            None
        }
    }
}

#[derive(serde::Deserialize)]
pub struct QueryQuery {
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
pub async fn query(
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
pub struct LastQuery {
    // Vec<u32> csv
    units: String,
}

#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnitData {
    pub produced_change: Option<i64>,
    pub last_status: Option<u32>,
    pub last_status_change: Option<i64>,
    pub previous_status: Option<u32>,
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
pub(crate) async fn last(
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
            changes.insert(unit, status_of(&data, *unit));
        }

        Ok(json!({ "changes": changes }))
    })
    .await
}

pub fn status_of(data: &Data, unit: u32) -> UnitData {
    let mut unit_data = UnitData::default();
    let mut produced_previous = None;
    let mut status_previous = None;
    for obs in data.inner.iter().rev() {
        let found = match obs
            .inner
            .binary_search_by_key(&unit, |crafting| crafting.unit_number)
        {
            Ok(found) => found,
            Err(_) => continue,
        };
        let found = &obs.inner[found];
        if unit_data.produced_change.is_none() {
            if produced_previous.is_some() && produced_previous != Some(found.products_complete) {
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
    unit_data
}
