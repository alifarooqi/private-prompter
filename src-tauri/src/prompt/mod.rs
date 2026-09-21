//! Meta-prompt template loader + Handlebars renderer.
//!
//! Templates are Markdown files with `{{handlebars}}` placeholders. They live
//! in two places:
//!
//!   * `assets/prompts/` — bundled defaults, compiled into the binary via
//!     `include_str!`. Shipped with every install.
//!   * `~/Library/Application Support/PrivatePrompter/templates/` — user
//!     overrides; loaded at runtime and take precedence over the bundled
//!     defaults when names collide.

use std::path::PathBuf;

use handlebars::Handlebars;
use serde::Serialize;

use crate::context::ContextProfile;
use crate::model::store as store_paths;

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("template '{0}' not found")]
    NotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("render: {0}")]
    Render(#[from] handlebars::RenderError),
    #[error("template parse: {0}")]
    Template(#[from] handlebars::TemplateError),
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptInput<'a> {
    pub input: &'a str,
    pub context: &'a ContextProfile,
    pub profile_domain: &'a str,
    pub profile_tone: &'a str,
    pub profile_tools: Vec<String>,
}

const PROMPT_MASTER: &str = include_str!("../../../assets/prompts/prompt-master.md");
const UNIVERSAL: &str = include_str!("../../../assets/prompts/universal.md");
const CONCISE: &str = include_str!("../../../assets/prompts/concise.md");

/// Built-in templates, in the order they show up in Settings → Templates.
pub fn builtin_templates() -> Vec<TemplateSummary> {
    vec![
        TemplateSummary {
            id: "prompt-master".to_string(),
            display_name: "Prompt Master (default)".to_string(),
            source: TemplateSource::Builtin,
            description: "Transforms raw thought into a structured prompt using XML tags."
                .to_string(),
        },
        TemplateSummary {
            id: "universal".to_string(),
            display_name: "Universal".to_string(),
            source: TemplateSource::Builtin,
            description: "Tight rewrite for any text — no assumptions about domain.".to_string(),
        },
        TemplateSummary {
            id: "concise".to_string(),
            display_name: "Concise".to_string(),
            source: TemplateSource::Builtin,
            description: "Shorten without losing meaning.".to_string(),
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateSummary {
    pub id: String,
    pub display_name: String,
    pub source: TemplateSource,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateSource {
    Builtin,
    User,
}

/// Resolve a template by id. User templates take precedence over builtins.
pub fn load_template(id: &str) -> Result<(String, String), PromptError> {
    if let Some(user) = load_user_template(id)? {
        return Ok((id.to_string(), user));
    }
    match id {
        "prompt-master" => Ok((id.to_string(), PROMPT_MASTER.to_string())),
        "universal" => Ok((id.to_string(), UNIVERSAL.to_string())),
        "concise" => Ok((id.to_string(), CONCISE.to_string())),
        _ => Err(PromptError::NotFound(id.to_string())),
    }
}

fn load_user_template(id: &str) -> std::io::Result<Option<String>> {
    let path: PathBuf = store_paths::templates_dir().join(format!("{id}.md"));
    if path.exists() {
        Ok(Some(std::fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}

/// Render a template string against the given input. Returns the final
/// prompt we send to the LLM.
pub fn render(template: &str, input: &PromptInput) -> Result<String, PromptError> {
    let mut hb = Handlebars::new();
    hb.register_template_string("tmpl", template)?;
    let rendered = hb.render("tmpl", input)?;
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_builtins() {
        for id in ["prompt-master", "universal", "concise"] {
            assert!(load_template(id).is_ok());
        }
    }

    #[test]
    fn unknown_template_errors() {
        assert!(matches!(
            load_template("nope"),
            Err(PromptError::NotFound(_))
        ));
    }

    #[test]
    fn renders_handlebars_placeholders() {
        let profile = ContextProfile::universal();
        let input = PromptInput {
            input: "hello",
            context: &profile,
            profile_domain: &profile.domain,
            profile_tone: &profile.tone,
            profile_tools: profile.tools.clone(),
        };
        let rendered = render("domain={{profile_domain}} input={{input}}", &input).unwrap();
        assert_eq!(rendered, "domain=universal input=hello");
    }
}
