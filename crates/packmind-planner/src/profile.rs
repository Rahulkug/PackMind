//! Task modes: named retrieval biases. A mode is data — edge priors, role
//! scores, keyword bias — applied on top of the repo's configured weights.
//! Every bias remains visible in pack `why` fields (decomposability contract).

use packmind_core::config::{Config, ScoringWeights};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Default,
    Bugfix,
    Refactor,
    Test,
    Security,
    Architecture,
    Pr,
}

pub const MODE_NAMES: &[&str] = &[
    "default",
    "bugfix",
    "refactor",
    "test",
    "security",
    "architecture",
    "pr",
];

impl Mode {
    pub fn parse(s: &str) -> anyhow::Result<Mode> {
        Ok(match s {
            "" | "default" => Mode::Default,
            "bugfix" => Mode::Bugfix,
            "refactor" => Mode::Refactor,
            "test" => Mode::Test,
            "security" => Mode::Security,
            "architecture" => Mode::Architecture,
            "pr" => Mode::Pr,
            other => anyhow::bail!(
                "unknown mode '{other}' (expected one of: {})",
                MODE_NAMES.join("|")
            ),
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Mode::Default => "default",
            Mode::Bugfix => "bugfix",
            Mode::Refactor => "refactor",
            Mode::Test => "test",
            Mode::Security => "security",
            Mode::Architecture => "architecture",
            Mode::Pr => "pr",
        }
    }
}

/// Per-`why.reason` score priors (the `edge_prior` component).
#[derive(Debug, Clone)]
pub struct EdgePriors {
    pub anchor: f64,
    pub calls: f64,     // calls | called_by
    pub tested_by: f64,
    pub structure: f64, // inherits | imports | imported_by
    pub doc: f64,       // doc_mention
    pub other: f64,     // search_hit and anything else
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub mode: Mode,
    pub weights: ScoringWeights,
    pub priors: EdgePriors,
    /// Role component value for test-role vs other chunks.
    pub role_test: f64,
    pub role_code: f64,
    /// Score bonus when path/symbol matches any keyword (lowercased
    /// substring). The matched keyword is appended to the item's `why.detail`.
    pub keywords: &'static [&'static str],
    pub keyword_bonus: f64,
    /// Anchor files that changed since the last index (bugfix/pr modes).
    pub anchor_dirty: bool,
    /// Prune candidates scoring below this; anchors are exempt.
    pub threshold: f64,
}

const SECURITY_KEYWORDS: &[&str] = &[
    "auth", "token", "secret", "session", "password", "permission", "crypt",
    "valid", "sanitiz", "credential",
];

impl Profile {
    pub fn new(mode: Mode, config: &Config) -> Profile {
        let w = config.scoring.clone();
        let base = EdgePriors {
            anchor: 1.0,
            calls: 0.8,
            tested_by: 0.8,
            structure: 0.6,
            doc: 0.4,
            other: 0.5,
        };
        let mut p = Profile {
            mode,
            weights: w,
            priors: base,
            role_test: 0.5,
            role_code: 0.7,
            keywords: &[],
            keyword_bonus: 0.0,
            anchor_dirty: false,
            threshold: config.plan.threshold,
        };
        match mode {
            Mode::Default => {}
            Mode::Bugfix => {
                p.priors.tested_by = 1.0;
                p.priors.calls = 0.9;
                p.role_test = 0.9;
                p.anchor_dirty = true;
            }
            Mode::Refactor => {
                p.priors.calls = 0.9;
                p.priors.structure = 0.9;
                p.priors.doc = 0.3;
                p.role_test = 0.6;
                p.role_code = 0.8;
            }
            Mode::Test => {
                p.priors.tested_by = 1.0;
                p.role_test = 1.0;
                p.role_code = 0.6;
            }
            Mode::Security => {
                p.priors.tested_by = 0.7;
                p.keywords = SECURITY_KEYWORDS;
                p.keyword_bonus = 0.15;
            }
            Mode::Architecture => {
                p.priors.doc = 1.0;
                p.priors.structure = 0.9;
                p.role_test = 0.3;
                // Structure matters more than lexical luck for orientation.
                p.weights.centrality = (p.weights.centrality * 2.0).min(1.0);
                p.weights.text = (p.weights.text - 0.10).max(0.0);
            }
            Mode::Pr => {
                p.priors.tested_by = 0.9;
                p.priors.doc = 0.5;
                p.role_test = 0.8;
                p.anchor_dirty = true;
            }
        }
        p
    }

    pub fn edge_prior(&self, reason: &str) -> f64 {
        match reason {
            "anchor" => self.priors.anchor,
            "calls" | "called_by" => self.priors.calls,
            "tested_by" => self.priors.tested_by,
            "inherits" | "imports" | "imported_by" => self.priors.structure,
            "doc_mention" => self.priors.doc,
            _ => self.priors.other,
        }
    }

    pub fn role_score(&self, role: &str) -> f64 {
        if role == "test" {
            self.role_test
        } else {
            self.role_code
        }
    }

    /// First keyword (if any) matching the node's path or symbol.
    pub fn keyword_hit(&self, path: &str, symbol: Option<&str>) -> Option<&'static str> {
        if self.keywords.is_empty() {
            return None;
        }
        let path = path.to_lowercase();
        let symbol = symbol.map(|s| s.to_lowercase()).unwrap_or_default();
        self.keywords
            .iter()
            .find(|k| path.contains(**k) || symbol.contains(**k))
            .copied()
    }
}
