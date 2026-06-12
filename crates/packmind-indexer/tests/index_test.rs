//! Integration tests: indexing, incremental invalidation (the freshness
//! claim as an executable test), edges, and planner determinism.

use packmind_core::model::NodeKind;
use packmind_core::Store;
use packmind_indexer::{index_repo, IndexOptions};
use std::fs;
use std::path::Path;

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn fixture(root: &Path) {
    write(
        root,
        "fx.py",
        "class FxRateService:\n    \"\"\"Rates.\"\"\"\n\n    def get_rate(self, base, quote):\n        return 1.0\n\n    def convert(self, amount, base, quote):\n        return amount * self.get_rate(base, quote)\n",
    );
    write(
        root,
        "payments.py",
        "from fx import FxRateService\n\n\nclass PaymentValidator:\n    \"\"\"Validates payments.\"\"\"\n\n    def __init__(self, fx):\n        self.fx = fx\n\n    def validate(self, payment):\n        usd = self.fx.convert(payment[\"amount\"], payment[\"currency\"], \"USD\")\n        assert usd > 0\n\n\ndef process_payment(validator, payment):\n    validator.validate(payment)\n    return \"ok\"\n",
    );
    write(
        root,
        "tests/test_payments.py",
        "from payments import PaymentValidator, process_payment\n\n\ndef test_validate():\n    v = PaymentValidator(None)\n    process_payment(v, {\"amount\": 1, \"currency\": \"USD\"})\n",
    );
    write(
        root,
        "README.md",
        "# Demo\n\nPaymentValidator validates payments using FxRateService.\n",
    );
}

#[test]
fn index_builds_chunks_signatures_and_edges() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let mut store = Store::open(dir.path()).unwrap();
    let report = index_repo(&mut store, &IndexOptions::default()).unwrap();

    assert!(report.chunks_new >= 4, "expected chunks, got {report:?}");
    let counts = store.counts().unwrap();
    assert!(counts.signatures >= 4);
    assert!(counts.docs >= 1, "README should produce doc chunks");
    assert!(counts.edges >= 2, "expected call/test/import edges");

    // Symbol lookup + signature substitution material exist.
    let v = store.find_symbol("PaymentValidator", 1).unwrap();
    assert_eq!(v.len(), 1);
    assert!(store
        .signature_for(&v[0].path, "PaymentValidator")
        .is_some());
}

#[test]
fn comment_only_change_preserves_sibling_chunks() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let mut store = Store::open(dir.path()).unwrap();
    index_repo(&mut store, &IndexOptions::default()).unwrap();

    let before: Vec<_> = store
        .nodes_by_path("payments.py", &[NodeKind::AstChunk])
        .unwrap()
        .iter()
        .map(|n| n.id)
        .collect();
    assert!(before.len() >= 2);

    // Prepend a module comment: every chunk's bytes shift, but their content
    // (and therefore identity and cache position) is unchanged.
    let original = fs::read_to_string(dir.path().join("payments.py")).unwrap();
    fs::write(
        dir.path().join("payments.py"),
        format!("# updated header comment\n{original}"),
    )
    .unwrap();

    let report = index_repo(&mut store, &IndexOptions { force: true }).unwrap();
    let after: Vec<_> = store
        .nodes_by_path("payments.py", &[NodeKind::AstChunk])
        .unwrap()
        .iter()
        .map(|n| n.id)
        .collect();

    assert_eq!(before.len(), after.len());
    for id in &before {
        assert!(
            after.contains(id),
            "chunk identity must survive a comment-only change"
        );
    }
    assert_eq!(
        report.chunks_staled, 0,
        "no chunk should be invalidated: {report:?}"
    );
}

#[test]
fn editing_one_function_invalidates_only_it() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let mut store = Store::open(dir.path()).unwrap();
    index_repo(&mut store, &IndexOptions::default()).unwrap();

    let original = fs::read_to_string(dir.path().join("payments.py")).unwrap();
    let edited = original.replace("return \"ok\"", "return \"settled\"");
    assert_ne!(original, edited);
    fs::write(dir.path().join("payments.py"), edited).unwrap();

    let report = index_repo(&mut store, &IndexOptions { force: true }).unwrap();
    assert_eq!(
        report.chunks_staled, 1,
        "only process_payment should invalidate: {report:?}"
    );
    assert_eq!(report.chunks_new, 1);
    assert!(
        report.chunks_preserved >= 1,
        "PaymentValidator must be preserved"
    );
}

#[test]
fn reindex_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let mut store = Store::open(dir.path()).unwrap();
    let first = index_repo(&mut store, &IndexOptions::default()).unwrap();
    let second = index_repo(&mut store, &IndexOptions { force: true }).unwrap();
    assert_eq!(
        second.chunks_new, 0,
        "second run must create nothing: {second:?}"
    );
    assert_eq!(second.chunks_staled, 0);
    assert_eq!(second.chunks_preserved, first.chunks_new);
}
