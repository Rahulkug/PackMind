//! Repo configuration: `.packmind/config.toml`. Zero-config by design —
//! every field has a default and an absent file is a valid config.
//! Resolution order (LLD §7.3.3): CLI flag > MCP arg > config > default.

use serde::Deserialize;
use std::path::Path;

pub const CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_BUDGET: i64 = 12_000;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub scoring: ScoringWeights,
    pub plan: PlanConfig,
}

/// Relevance score components (§7.2.2). Weights are config, not code:
/// `score = text·w_text + hop·w_hop + edge·w_edge + centrality·w_centrality
///          + role·w_role`. Task modes start from these and re-bias priors.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScoringWeights {
    pub text: f64,
    pub hop: f64,
    pub edge_prior: f64,
    pub centrality: f64,
    pub role: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        ScoringWeights {
            text: 0.40,
            hop: 0.25,
            edge_prior: 0.15,
            centrality: 0.10,
            role: 0.10,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PlanConfig {
    /// Default token budget when no flag/arg is given.
    pub budget: i64,
    /// Candidates below this final score are pruned (anchors are never
    /// pruned). 0.0 disables pruning.
    pub threshold: f64,
}

impl Default for PlanConfig {
    fn default() -> Self {
        PlanConfig {
            budget: DEFAULT_BUDGET,
            threshold: 0.0,
        }
    }
}

impl Config {
    /// Load `.packmind/config.toml` under `root`; defaults if absent.
    /// A malformed file is an error (silent fallback would hide typos).
    pub fn load(root: &Path) -> anyhow::Result<Config> {
        let path = root.join(crate::STATE_DIR).join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("invalid {}: {e}", path.display()))
    }
}

/// Commented template written by `packmind init` (only if absent).
pub const TEMPLATE: &str = "\
# PackMind configuration. Every value is optional; defaults shown.

[scoring]            # relevance score component weights
# text = 0.40        # lexical match strength
# hop = 0.25         # graph distance from anchors/hits
# edge_prior = 0.15  # edge-type prior (tests/callers > docs)
# centrality = 0.10  # repo-structural importance
# role = 0.10        # file role (test/config/code)

[plan]
# budget = 12000     # default token budget for packs
# threshold = 0.0    # prune candidates scoring below this (anchors exempt)
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_absent_and_template_parses() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.plan.budget, DEFAULT_BUDGET);
        assert_eq!(cfg.scoring.text, 0.40);
        // The shipped template must itself be valid config.
        let parsed: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(parsed.plan.threshold, 0.0);
    }

    #[test]
    fn partial_override_keeps_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join(crate::STATE_DIR);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join(CONFIG_FILE),
            "[scoring]\ntext = 0.6\n[plan]\nbudget = 8000\n",
        )
        .unwrap();
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.scoring.text, 0.6);
        assert_eq!(cfg.scoring.hop, 0.25); // untouched default
        assert_eq!(cfg.plan.budget, 8000);
    }
}
