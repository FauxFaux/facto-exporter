use anyhow::ensure;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use facto_exporter::Observation;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::by_unit::split_units;
use crate::{okay_or_500, AppState};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct LongQuery {
    // Vec<u32> csv
    units: String,
    steps: usize,
    #[serde(with = "time::serde::rfc3339")]
    start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    end: OffsetDateTime,
}

#[axum::debug_handler]
pub(crate) async fn long(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LongQuery>,
) -> impl IntoResponse {
    okay_or_500(&state.logger, || async {
        ensure!(query.steps > 0, "steps must be greater than 0");
        let units = split_units(&state.logger, &query.units)
            .ok_or_else(|| anyhow::anyhow!("invalid units"))?;
        let data = state.data.read().await;
        let mut obs = data.inner.iter().collect::<Vec<_>>();
        obs.sort_unstable_by_key(|date| date.ts());
        obs.dedup_by_key(|date| date.ts());
        let start_idx =
            match obs.binary_search_by_key(&query.start.unix_timestamp(), |date| date.ts()) {
                Ok(p) => p,
                Err(p) => p,
            };
        let end_idx = match obs.binary_search_by_key(&query.end.unix_timestamp(), |date| date.ts())
        {
            Ok(p) => p,
            Err(p) => p,
        };

        let obs = &obs[start_idx..end_idx];
        let step = obs.len() / query.steps;

        // let t = obs.chunks(step).map(|v| v.iter().map(|v| v.time.format(&Rfc3339).expect("static format")).collect::<Vec<_>>()).collect::<Vec<_>>();

        ensure!(step > 0, "step too small, {} / {}", obs.len(), query.steps);
        let step_obs = obs.chunks(step).collect::<Vec<_>>();
        ensure!(step_obs.len() > 1, "not enough observations");

        let steps = stepper(&step_obs, &units)?;
        let summary = step_obs
            .iter()
            .map(|step| GeneralOutput {
                observations: step.len(),
                dates: vec![format(&step[0].time), format(&step[step.len() - 1].time)],
            })
            .collect::<Vec<_>>();

        Ok(json!({ "units": units, "summary": summary, "steps": steps }))
    })
    .await
}

#[derive(serde::Serialize)]
struct GeneralOutput {
    #[serde(rename = "o")]
    observations: usize,
    #[serde(rename = "ds")]
    dates: Vec<String>,
}

#[derive(serde::Serialize)]
struct UnitOutput {
    #[serde(rename = "s")]
    statuses: HashMap<u8, usize>,
    #[serde(rename = "p")]
    products: u32,
}

fn stepper(steps: &[&[&Observation]], units: &[u32]) -> Result<Vec<Vec<UnitOutput>>> {
    // most of these hashmaps from unit number could be fixed by remembering the idxes

    let mut by_step = Vec::with_capacity(steps.len());
    let mut prev = HashMap::with_capacity(units.len());
    let units_lookup: HashSet<u32> = units.iter().copied().collect();
    for step in steps {
        // unit -> (status, count)
        let mut step_statuses: HashMap<u32, HashMap<u8, usize>> =
            HashMap::with_capacity(units.len());
        // unit -> total production this step, if previously seen
        let mut step_products: HashMap<u32, u32> = HashMap::with_capacity(units.len());
        for (i, obs) in step.iter().rev().enumerate() {
            for crafting in &obs.inner {
                if !units_lookup.contains(&crafting.unit_number) {
                    continue;
                }

                *step_statuses
                    .entry(crafting.unit_number)
                    .or_default()
                    .entry(u8::try_from(crafting.status)?)
                    .or_default() += 1;

                if 0 != i {
                    continue;
                }
                if let Some(prev) = prev.get(&crafting.unit_number) {
                    step_products.insert(crafting.unit_number, crafting.products_complete - prev);
                }

                prev.insert(crafting.unit_number, crafting.products_complete);
            }
        }
        by_step.push(
            units
                .iter()
                .map(|unit| UnitOutput {
                    statuses: step_statuses.remove(unit).unwrap_or_default(),
                    products: step_products.remove(unit).unwrap_or_default(),
                })
                .collect::<Vec<_>>(),
        );
    }
    Ok(by_step)
}

fn format(time: &OffsetDateTime) -> String {
    time.replace_millisecond(0)
        .expect("replace millisecond")
        .replace_nanosecond(0)
        .expect("replace nanosecond")
        .format(&Rfc3339)
        .expect("static format")
}
