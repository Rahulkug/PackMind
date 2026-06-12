//! End-to-end pack tests on the bundled example repo: relevance, savings,
//! explain contract, and planner determinism.

use packmind_core::Store;
use packmind_indexer::{index_repo, IndexOptions};
use packmind_planner::plan::{build_pack, PackRequest};
use std::path::PathBuf;

fn example_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/small-python-service")
        .canonicalize()
        .unwrap()
}

fn indexed_copy() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let src = example_repo();
    for entry in walkdir(&src) {
        let rel = entry.strip_prefix(&src).unwrap();
        if rel.starts_with(packmind_core::STATE_DIR) {
            continue;
        }
        let to = dir.path().join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(&entry, &to).unwrap();
    }
    let mut store = Store::open(dir.path()).unwrap();
    index_repo(&mut store, &IndexOptions::default()).unwrap();
    (dir, store)
}

fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

fn request(query: &str) -> PackRequest {
    PackRequest {
        query: query.to_string(),
        token_budget: 4000,
        include_content: false,
        dirty_paths: vec![],
        mode: String::new(),
        surface: "cli".to_string(),
    }
}

#[test]
fn pack_finds_relevant_code_with_explains_and_savings() {
    let (_dir, store) = indexed_copy();
    let pack = build_pack(
        &store,
        &request("Refactor PaymentValidator to use FxRateService"),
    )
    .unwrap();

    assert!(!pack.items.is_empty());
    let paths: Vec<&str> = pack.items.iter().map(|i| i.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.contains("payments.py")),
        "items: {paths:?}"
    );

    // Explain contract: every item carries a reason.
    for item in &pack.items {
        assert!(!item.why.reason.is_empty());
        assert!(!item.why.detail.is_empty());
    }
    // Savings against the file-dump counterfactual.
    assert!(pack.totals.estimated_raw_tokens > pack.totals.selected_tokens);
    assert!(pack.totals.saved_pct > 0.0);
    assert!(pack.totals.selected_tokens <= pack.token_budget);
}

#[test]
fn planner_is_deterministic() {
    let (_dir, store) = indexed_copy();
    let a = build_pack(&store, &request("where is payment validation enforced")).unwrap();
    let b = build_pack(&store, &request("where is payment validation enforced")).unwrap();
    let ids_a: Vec<&str> = a.items.iter().map(|i| i.node.as_str()).collect();
    let ids_b: Vec<&str> = b.items.iter().map(|i| i.node.as_str()).collect();
    assert_eq!(
        ids_a, ids_b,
        "same query + snapshot must select identical, identically-ordered items"
    );
}

#[test]
fn anchors_named_in_query_appear_in_full_when_budget_allows() {
    let (_dir, store) = indexed_copy();
    let pack = build_pack(
        &store,
        &request("Refactor PaymentValidator to use FxRateService"),
    )
    .unwrap();

    // Budget (4000) far exceeds the whole repo: both symbols named in the
    // query must arrive as full anchor chunks, not signature stand-ins.
    for symbol in ["PaymentValidator", "FxRateService"] {
        let item = pack
            .items
            .iter()
            .find(|i| i.symbol.as_deref() == Some(symbol))
            .unwrap_or_else(|| panic!("{symbol} missing from pack"));
        assert_eq!(item.item_type, "ast_chunk", "{symbol}: {:?}", item.why);
        assert_eq!(item.why.reason, "anchor", "{symbol}: {:?}", item.why);
    }
}

#[test]
fn pr_mode_anchors_dirty_files() {
    let (_dir, store) = indexed_copy();
    // A query that does not mention payments at all: only the dirty-file
    // anchor can pull payments.py in as an anchor.
    let pack = build_pack(
        &store,
        &PackRequest {
            dirty_paths: vec!["payments.py".to_string()],
            mode: "pr".to_string(),
            ..request("improve logging configuration")
        },
    )
    .unwrap();
    assert_eq!(pack.mode, "pr");
    assert_eq!(pack.freshness.state, "stale");
    let anchored = pack
        .items
        .iter()
        .find(|i| i.path == "payments.py" && i.why.reason == "anchor")
        .expect("dirty payments.py should be anchored in pr mode");
    assert!(anchored.why.detail.contains("changed in working tree"));

    // Default mode must NOT anchor dirty files.
    let default_pack = build_pack(
        &store,
        &PackRequest {
            dirty_paths: vec!["payments.py".to_string()],
            ..request("improve logging configuration")
        },
    )
    .unwrap();
    assert!(!default_pack
        .items
        .iter()
        .any(|i| i.path == "payments.py" && i.why.reason == "anchor"));
}

#[test]
fn security_mode_boost_is_visible_in_why() {
    let (_dir, store) = indexed_copy();
    let sec = build_pack(
        &store,
        &PackRequest {
            mode: "security".to_string(),
            ..request("review payment request handling")
        },
    )
    .unwrap();
    // auth.py items must carry the visible mode-boost annotation
    // (decomposability: no invisible score components).
    let boosted = sec
        .items
        .iter()
        .find(|i| i.path == "auth.py")
        .expect("security mode should surface auth.py");
    assert!(
        boosted.why.detail.contains("security mode: matches"),
        "detail: {}",
        boosted.why.detail
    );
}

#[test]
fn unknown_mode_is_rejected() {
    let (_dir, store) = indexed_copy();
    let err = build_pack(
        &store,
        &PackRequest {
            mode: "yolo".to_string(),
            ..request("anything")
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("unknown mode"));
}

#[test]
fn config_threshold_prunes_low_scores_but_never_anchors() {
    let (dir, store) = indexed_copy();
    let baseline = build_pack(
        &store,
        &request("Refactor PaymentValidator to use FxRateService"),
    )
    .unwrap();
    let state = dir.path().join(packmind_core::STATE_DIR);
    std::fs::write(state.join("config.toml"), "[plan]\nthreshold = 0.55\n").unwrap();
    let pruned = build_pack(
        &store,
        &request("Refactor PaymentValidator to use FxRateService"),
    )
    .unwrap();
    assert!(
        pruned.items.len() < baseline.items.len(),
        "threshold should prune walk noise ({} vs {})",
        pruned.items.len(),
        baseline.items.len()
    );
    for item in &pruned.items {
        assert!(
            item.why.reason == "anchor" || item.why.score.unwrap_or(0.0) >= 0.55,
            "non-anchor below threshold survived: {:?}",
            item.why
        );
    }
    // Anchors named in the query must survive any threshold.
    for symbol in ["PaymentValidator", "FxRateService"] {
        assert!(
            pruned.items.iter().any(|i| i.symbol.as_deref() == Some(symbol)),
            "{symbol} pruned"
        );
    }
}

#[test]
fn cache_report_reflects_recorded_packs() {
    let (_dir, store) = indexed_copy();
    build_pack(&store, &request("where is payment validation enforced")).unwrap();
    build_pack(&store, &request("explain authentication")).unwrap();
    let r = packmind_planner::report::cache_report(&store).unwrap();
    assert!(r["hot_set"]["version"].as_i64().unwrap_or(0) >= 1);
    assert!(r["hot_set"]["estimated_reusable_tokens"].as_i64().unwrap_or(0) > 0);
    assert_eq!(r["packs"]["analyzed"], 2);
    assert_eq!(r["packs"]["prefix_order_consistent"], 2);
    assert_eq!(r["cache_stability_score"], 1.0);
}

#[test]
fn budget_is_respected_with_signature_substitution() {
    let (_dir, store) = indexed_copy();
    let small = PackRequest {
        token_budget: 400,
        ..request("Refactor PaymentValidator to use FxRateService")
    };
    let pack = build_pack(&store, &small).unwrap();
    assert!(
        pack.totals.selected_tokens <= 400,
        "totals: {:?}",
        pack.totals
    );
}
