//! Context detection pipeline (revised for Phase 6).
//!
//! Tier 3 (the LLM classifier) was dropped — running a 1B model adds
//! hundreds of ms; heuristics cover the same surface for our supported apps.
//!
//!   * Tier 1: frontmost app name + (if browser) URL/title via Apple Events.
//!   * Tier 2: regex/keyword routing against URL fragments to pick a profile.
//!   * Tier 4 (fallback): Universal profile if everything else misses.

pub mod heuristics;
pub mod inspector;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextProfile {
    pub domain: String,
    pub tools: Vec<String>,
    pub tone: String,
    pub raw_evidence: String,
}

impl ContextProfile {
    pub fn universal() -> Self {
        Self {
            domain: "universal".to_string(),
            tools: vec![],
            tone: "neutral".to_string(),
            raw_evidence: "no app/url evidence".to_string(),
        }
    }
}

/// Frontmost application snapshot from Tier 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmostApp {
    pub name: String,
    pub bundle_id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Run the full pipeline. Returns a profile plus the raw evidence the user
/// can see in the About/Privacy panels if they care.
pub fn detect() -> ContextProfile {
    let app = inspector::frontmost_app().unwrap_or_else(|| FrontmostApp {
        name: "Unknown".to_string(),
        bundle_id: None,
        url: None,
        title: None,
    });

    if let Some(profile) = heuristics::match_url(app.url.as_deref().unwrap_or("")) {
        return profile;
    }

    if let Some(profile) = heuristics::match_app(&app.name) {
        return profile;
    }

    ContextProfile::universal()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_is_default() {
        let p = ContextProfile::universal();
        assert_eq!(p.domain, "universal");
    }
}