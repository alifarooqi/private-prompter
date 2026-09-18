# Changelog

All notable changes to PrivatePrompter will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial scaffold: Tauri v2 (React + TypeScript + Rust), tray-on-menu + permissions onboarding.
- Dynamic model management (RAM detection, HuggingFace downloader, model registry).
- Embedded `llama.cpp` sidecar for fully local inference.
- Global hotkey + clipboard pipeline with streaming, cancellation, and 10-second undo window.
- Context detection pipeline (Tier 1 OS inspection + Tier 2 heuristics + Tier 4 fallback).
- Bundled meta-prompt templates (default: `nidhinjs/prompt-master`, MIT, credited).
- GitHub Releases distribution with code signing + notarization.

### Deferred to v2
- Personal Intelligence layer: LanceDB indexing of past prompts and local documents for RAG-style prompt augmentation. See `plan/02.personal-intelligence.md`.

[Unreleased]: https://github.com/<owner>/private-prompter/compare/main...HEAD