//! Token counting. Exact counts via tiktoken (o200k_base) when available,
//! with a chars/4 fallback so the binary never fails on counting.

use once_cell::sync::Lazy;
use tiktoken_rs::CoreBPE;

static BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::o200k_base().ok());

pub const TOKENIZER_NAME: &str = "o200k_base";

pub fn count(text: &str) -> i64 {
    match BPE.as_ref() {
        Some(bpe) => bpe.encode_with_special_tokens(text).len() as i64,
        None => approx(text),
    }
}

pub fn approx(text: &str) -> i64 {
    (text.len() as i64 + 3) / 4
}

/// Whether counts are exact (tiktoken loaded) or estimated. Surfaced in the
/// pack contract as `token_estimate`.
pub fn is_exact() -> bool {
    BPE.is_some()
}
