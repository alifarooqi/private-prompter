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

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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

/// How long to reuse a previous context detection. The frontmost app
/// doesn't change during a hotkey session, so for repeated hotkey
/// presses within this window we skip the two `osascript` forks (which
/// cost 30–600ms each).
const CONTEXT_CACHE_TTL: Duration = Duration::from_secs(2);

/// Run the full pipeline. Returns a profile plus the raw evidence the user
/// can see in the About/Privacy panels if they care.
///
/// Cached briefly: frontmost app + URL are essentially static across
/// consecutive hotkey presses, so we avoid re-running the osascript
/// probes within `CONTEXT_CACHE_TTL`.
pub fn detect() -> ContextProfile {
    let now = Instant::now();
    {
        let cache = context_cache();
        if let Ok(guard) = cache.lock() {
            if let Some((profile, ts)) = guard.as_ref() {
                if now.duration_since(*ts) < CONTEXT_CACHE_TTL {
                    return profile.clone();
                }
            }
        }
    }
    let profile = detect_fresh();
    if let Ok(mut guard) = context_cache().lock() {
        *guard = Some((profile.clone(), now));
    }
    profile
}

fn detect_fresh() -> ContextProfile {
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

fn context_cache() -> &'static Mutex<Option<(ContextProfile, Instant)>> {
    static CACHE: OnceLock<Mutex<Option<(ContextProfile, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
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
