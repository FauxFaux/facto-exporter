use anyhow::anyhow;
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use serde_json::json;

use crate::by_unit::status_of;
use crate::{okay_or_500, AppState, KNOWN_STATUSES};

#[axum::debug_handler]
pub async fn metrics_raw(State(state): State<Arc<AppState>>) -> String {
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

#[axum::debug_handler]
pub async fn bulk_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    okay_or_500(&state.logger, || async {
        let data = state.data.read().await;
        let units = data
            .inner
            .last()
            .ok_or_else(|| anyhow!("service is empty"))?
            .inner
            .iter()
            .map(|c| c.unit_number)
            .collect::<Vec<_>>();
        // let mut statuses = HashMap::with_capacity(units.len());
        let statuses = units
            .iter()
            .copied()
            .map(|unit| {
                let s = status_of(&data, unit);
                (
                    unit,
                    (
                        s.produced_change,
                        s.last_status_change,
                        s.last_status,
                        s.previous_status,
                    ),
                )
            })
            .collect::<Vec<_>>();

        Ok(json!({"statuses": statuses}))
    })
    .await
}
