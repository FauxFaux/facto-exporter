use std::sync::Arc;

use axum::extract::State;

use crate::{AppState, KNOWN_STATUSES};

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
