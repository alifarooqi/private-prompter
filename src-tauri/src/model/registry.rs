//! Parses `assets/models.json` and exposes lookups.
//!
//! The manifest is bundled into the binary via `include_str!` so the app has
//! zero network calls to discover what models are available.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use super::ModelError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
    pub publisher: String,
    pub repo: String,
    pub file: String,
    pub url: String,
    pub size_bytes: u64,
    /// Hex-encoded SHA256. `00…00` is a placeholder pending real release
    /// verification; downloads with that hash are accepted as "not yet
    /// pinned" but logged so we don't ship to production this way.
    pub sha256: String,
    pub context_length: u32,
    pub min_ram_bytes: u64,
    pub recommended_for: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    models: Vec<ModelEntry>,
    default_model_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Registry {
    models: Vec<ModelEntry>,
    default_model_id: String,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

const MANIFEST_JSON: &str = include_str!("../../../assets/models.json");

pub fn get() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let parsed: Manifest =
            serde_json::from_str(MANIFEST_JSON).expect("bundled models.json is valid");
        Registry {
            models: parsed.models,
            default_model_id: parsed.default_model_id,
        }
    })
}

impl Registry {
    pub fn list(&self) -> &[ModelEntry] {
        &self.models
    }

    pub fn default(&self) -> &ModelEntry {
        self.models
            .iter()
            .find(|m| m.id == self.default_model_id)
            .expect("default_model_id is present in models list")
    }

    pub fn find(&self, id: &str) -> Result<&ModelEntry, ModelError> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| ModelError::UnknownModel(id.to_string()))
    }

    /// Pick the model whose `recommended_for` list contains this RAM tier.
    /// Falls back to the manifest's `default_model_id`.
    pub fn recommend_for_tier(&self, tier_key: &str) -> &ModelEntry {
        self.models
            .iter()
            .find(|m| m.recommended_for.iter().any(|r| r == tier_key))
            .unwrap_or_else(|| self.default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_default_present() {
        let reg = Registry {
            models: get().models.clone(),
            default_model_id: get().default_model_id.clone(),
        };
        assert!(!reg.list().is_empty());
        let default = reg.default();
        assert!(reg.list().iter().any(|m| m.id == default.id));
    }

    #[test]
    fn unknown_model_errors() {
        let reg = get();
        let err = reg.find("nonexistent-model-id").unwrap_err();
        matches!(err, ModelError::UnknownModel(_));
    }

    #[test]
    fn recommend_falls_back_to_default_for_unknown_tier() {
        let reg = get();
        let recommended = reg.recommend_for_tier("unknown");
        assert_eq!(recommended.id, reg.default().id);
    }
}