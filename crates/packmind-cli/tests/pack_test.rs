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
        stale_files: 0,
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
