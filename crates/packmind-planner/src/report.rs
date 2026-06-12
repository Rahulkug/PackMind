//! Cache-stability reporting: how reusable is this repo's rendered prefix,
//! and have recorded packs actually kept it stable? Powers `packmind
//! cache-report` and the dashboard's cache panel.

use anyhow::Result;
use packmind_core::model::id_hex;
use packmind_core::Store;
use serde_json::{json, Value};

/// True if `prefix` items appear in `order` in the same relative order.
fn is_subsequence(prefix: &[String], order: &[String]) -> bool {
    let mut it = order.iter();
    prefix.iter().all(|p| it.any(|o| o == p))
}

pub fn cache_report(store: &Store) -> Result<Value> {
    let (version, hot_ids) = store.hot_set()?;
    let hot_hex: Vec<String> = hot_ids.iter().map(id_hex).collect();

    let mut prefix_bytes = 0usize;
    let mut reusable_tokens = 0i64;
    let mut live_members = 0usize;
    for id in &hot_ids {
        if let Some(n) = store.get_node(id) {
            if n.valid {
                live_members += 1;
                reusable_tokens += n.tokens;
                prefix_bytes += crate::render::envelope_for_node(&n).len();
            }
        }
    }

    // Last index run: how many chunks survived the most recent change.
    let last_index = store
        .meta_get("last_index_report")
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    let index_stability = last_index.as_ref().map(|r| {
        let preserved = r["chunks_preserved"].as_i64().unwrap_or(0);
        let staled = r["chunks_staled"].as_i64().unwrap_or(0);
        if preserved + staled == 0 {
            100.0
        } else {
            (10_000.0 * preserved as f64 / (preserved + staled) as f64).round() / 100.0
        }
    });

    // Recorded packs: did they share the current prefix, in prefix order?
    let mut total = 0usize;
    let mut on_current_version = 0usize;
    let mut order_ok = 0usize;
    let mut saved_pcts: Vec<f64> = vec![];
    for (pack_json, _surface) in store.recent_packs(50)? {
        let Ok(p) = serde_json::from_str::<Value>(&pack_json) else {
            continue;
        };
        total += 1;
        if let Some(pct) = p["totals"]["saved_pct"].as_f64() {
            saved_pcts.push(pct);
        }
        let same_version = p["layout"]["hot_set_version"].as_i64() == Some(version);
        if same_version {
            on_current_version += 1;
            let prefix: Vec<String> = p["layout"]["stable_prefix_items"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if is_subsequence(&prefix, &hot_hex) {
                order_ok += 1;
            }
        }
    }
    let stability_score = if total == 0 {
        1.0
    } else {
        (order_ok as f64 / total as f64 * 100.0).round() / 100.0
    };
    saved_pcts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_saved = if saved_pcts.is_empty() {
        None
    } else {
        Some(saved_pcts[saved_pcts.len() / 2])
    };

    Ok(json!({
        "hot_set": {
            "version": version,
            "members": hot_ids.len(),
            "live_members": live_members,
            "stable_prefix_bytes": prefix_bytes,
            "estimated_reusable_tokens": reusable_tokens,
        },
        "last_index": {
            "chunk_preservation_pct": index_stability,
        },
        "packs": {
            "analyzed": total,
            "on_current_hot_set": on_current_version,
            "prefix_order_consistent": order_ok,
            "median_saved_pct": median_saved,
        },
        "cache_stability_score": stability_score,
    }))
}
