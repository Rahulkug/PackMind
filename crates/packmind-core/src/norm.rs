//! PM-NORM-1 content normalization.
//!
//! Contract (see docs/lld.md §5.1):
//! 1. Must decode as UTF-8; otherwise the file is excluded from the graph.
//! 2. Strip a leading U+FEFF BOM.
//! 3. CRLF and CR -> LF.
//! 4. Strip trailing spaces/tabs on each line.
//! 5. Exactly one trailing newline.
//! 6. Tabs, interior whitespace, and comments are preserved (comments are content).
//! 7. No Unicode normalization.
//!
//! Invariant: the renderer serves these normalized bytes themselves, so
//! hash-input == served-output and prefix stability holds by construction.

/// Normalize raw file bytes. Returns `None` when the bytes are not valid UTF-8.
pub fn pm_norm_1(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    let s = s.strip_prefix('\u{feff}').unwrap_or(s);
    let s = s.replace("\r\n", "\n").replace('\r', "\n");

    let mut out = String::with_capacity(s.len() + 1);
    for line in s.split('\n') {
        out.push_str(line.trim_end_matches([' ', '\t']));
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent() {
        let cases: &[&str] = &[
            "def f():\r\n    pass   \r\n\r\n\r\n",
            "\u{feff}hello\rworld",
            "",
            "no trailing newline",
            "tabs\tkept\t\n",
        ];
        for c in cases {
            let once = pm_norm_1(c.as_bytes()).unwrap();
            let twice = pm_norm_1(once.as_bytes()).unwrap();
            assert_eq!(once, twice, "norm must be idempotent for {c:?}");
        }
    }

    #[test]
    fn rules() {
        assert_eq!(pm_norm_1(b"a  \r\nb\r").unwrap(), "a\nb\n");
        assert_eq!(pm_norm_1("\u{feff}x".as_bytes()).unwrap(), "x\n");
        assert_eq!(pm_norm_1(b"x\n\n\n").unwrap(), "x\n");
        // interior whitespace and comments preserved
        assert_eq!(
            pm_norm_1(b"a\t b  # comment\n").unwrap(),
            "a\t b  # comment\n"
        );
        // non-UTF-8 rejected
        assert!(pm_norm_1(&[0xff, 0xfe, 0x00]).is_none());
    }
}
