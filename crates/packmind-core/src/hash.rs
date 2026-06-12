use crate::model::{NodeId, NodeKind};
use sha2::{Digest, Sha256};

/// Content hash preimage:
/// `"PM-NORM-1" 0x00 kind(1B) 0x00 lang 0x00 normalized_bytes`
///
/// `kind` and `lang` in the preimage prevent cross-type collisions
/// (e.g. a signature whose text equals some doc chunk).
pub fn content_hash(kind: NodeKind, lang: &str, normalized: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(crate::NORM_VERSION.as_bytes());
    h.update([0u8]);
    h.update([kind.as_i64() as u8]);
    h.update([0u8]);
    h.update(lang.as_bytes());
    h.update([0u8]);
    h.update(normalized.as_bytes());
    h.finalize().into()
}

pub fn node_id(hash: &[u8; 32]) -> NodeId {
    hash[..16].try_into().expect("hash is 32 bytes")
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_kind_separated() {
        let a = content_hash(NodeKind::AstChunk, "py", "def f():\n    pass\n");
        let b = content_hash(NodeKind::AstChunk, "py", "def f():\n    pass\n");
        assert_eq!(a, b);
        let c = content_hash(NodeKind::Signature, "py", "def f():\n    pass\n");
        assert_ne!(a, c, "kind must separate hash spaces");
        let d = content_hash(NodeKind::AstChunk, "ts", "def f():\n    pass\n");
        assert_ne!(a, d, "lang must separate hash spaces");
    }
}
