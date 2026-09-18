//! Tier 2 — keyword/URL routing into a context profile.
//!
//! The profiles below are what the meta-prompt template uses to bias the
//! output. Add new entries here when a new app/domain needs custom behavior.

use super::ContextProfile;

pub fn match_url(url: &str) -> Option<ContextProfile> {
    let lower = url.to_ascii_lowercase();

    let profile = if lower.contains("github.com")
        || lower.contains("gitlab.com")
        || lower.contains("bitbucket.org")
        || lower.contains("code.visualstudio.com")
        || lower.contains("cursor.com")
    {
        Some(ContextProfile {
            domain: "coding".to_string(),
            tools: vec![
                "git".to_string(),
                "shell".to_string(),
                "tests".to_string(),
            ],
            tone: "concise-technical".to_string(),
            raw_evidence: format!("url matched coding: {url}"),
        })
    } else if lower.contains("figma.com") || lower.contains("sketch.com") {
        Some(ContextProfile {
            domain: "design".to_string(),
            tools: vec!["figma".to_string()],
            tone: "visual-descriptive".to_string(),
            raw_evidence: format!("url matched design: {url}"),
        })
    } else if lower.contains("docs.google.com") || lower.contains("notion.so") {
        Some(ContextProfile {
            domain: "writing".to_string(),
            tools: vec![],
            tone: "polished-prose".to_string(),
            raw_evidence: format!("url matched writing: {url}"),
        })
    } else if lower.contains("mail.google.com")
        || lower.contains("outlook.live.com")
        || lower.contains("mail.apple.com")
    {
        Some(ContextProfile {
            domain: "email".to_string(),
            tools: vec![],
            tone: "professional-concise".to_string(),
            raw_evidence: format!("url matched email: {url}"),
        })
    } else {
        None
    };
    profile
}

pub fn match_app(app_name: &str) -> Option<ContextProfile> {
    let lower = app_name.to_ascii_lowercase();
    let profile = if lower.contains("code")
        || lower.contains("xcode")
        || lower.contains("terminal")
        || lower.contains("iterm")
    {
        Some(ContextProfile {
            domain: "coding".to_string(),
            tools: vec!["shell".to_string(), "tests".to_string()],
            tone: "concise-technical".to_string(),
            raw_evidence: format!("app matched coding: {app_name}"),
        })
    } else if lower.contains("pages") || lower.contains("scrivener") {
        Some(ContextProfile {
            domain: "writing".to_string(),
            tools: vec![],
            tone: "polished-prose".to_string(),
            raw_evidence: format!("app matched writing: {app_name}"),
        })
    } else if lower.contains("mail") {
        Some(ContextProfile {
            domain: "email".to_string(),
            tools: vec![],
            tone: "professional-concise".to_string(),
            raw_evidence: format!("app matched email: {app_name}"),
        })
    } else {
        None
    };
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_url_routes_to_coding() {
        let p = match_url("https://github.com/foo/bar/pull/1").unwrap();
        assert_eq!(p.domain, "coding");
    }

    #[test]
    fn figma_url_routes_to_design() {
        let p = match_url("https://www.figma.com/file/abc123").unwrap();
        assert_eq!(p.domain, "design");
    }

    #[test]
    fn unknown_url_returns_none() {
        assert!(match_url("https://example.com/").is_none());
    }

    #[test]
    fn vscode_app_routes_to_coding() {
        let p = match_app("Visual Studio Code").unwrap();
        assert_eq!(p.domain, "coding");
    }
}