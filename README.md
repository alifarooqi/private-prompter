# PrivatePrompter

> **Privacy-first, on-device prompt optimizer for macOS.** Highlight any text, press a hotkey, get a better prompt — without your text ever leaving your machine.

![Status](https://img.shields.io/badge/status-MVP-yellow)
![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-macOS-lightgrey)

PrivatePrompter is a standalone macOS menu-bar app. You highlight text in any application, press `⌘⇧Space`, and a small local LLM rewrites the highlighted text into a structured, well-formed prompt. The original text is replaced; an undo notification lets you revert in 10 seconds.

Everything happens locally after the first-run model download. **No telemetry. No cloud calls. No accounts.**

---

## Why

Prompting is a skill, and most of us don't have time to perfect every draft. Existing solutions either send your text to a server (privacy risk) or require a complex local LLM setup (steep barrier). PrivatePrompter collapses both: install the `.dmg`, grant Accessibility, done.

## Features (MVP)

- **Global hotkey** (`⌘⇧Space`, configurable) — works in any app.
- **Local LLM** — embedded `llama.cpp` running a small GGUF model (default ~1.5B Q4_K_M, ~1 GB).
- **Meta-prompt templates** — switch between bundled templates or write your own.
- **Context-aware** — detects the frontmost app (e.g. GitHub, Figma, Notion) and adapts the prompt template.
- **Streaming** — output appears progressively, no spinner-then-dump.
- **Undo** — every rewrite is reversible from a notification or `⌘⇧Z` for 10 seconds.

## Coming in v2

- Personal Intelligence layer — index your past prompts and local docs to inject highly-personalized context (see `plan/02.personal-intelligence.md`).

## Installation

Download the latest `.dmg` from the [Releases](https://github.com/<owner>/private-prompter/releases) page. Open it, drag PrivatePrompter to Applications, launch. macOS will ask you to grant **Accessibility** permission the first time you trigger a rewrite.

> The `.dmg` is signed and notarized by an Apple-issued Developer ID certificate.

## How it works

1. You press `⌘⇧Space` while text is highlighted.
2. PrivatePrompter copies the highlighted text to the clipboard, then asks your local `llama.cpp` server (running as a Tauri sidecar) to rewrite it using the active meta-prompt template.
3. Streamed output replaces the highlighted text via simulated paste.
4. A notification appears offering **Undo** (`⌘⇧Z` or click the button), valid for 10 seconds.

See [`docs/architecture.md`](docs/architecture.md) for the full picture.

## Privacy

**PrivatePrompter makes no network calls after the initial model download.** No analytics, no telemetry, no update pings beyond a manual "Check for updates" action. Your highlighted text never leaves your device. See [`docs/privacy.md`](docs/privacy.md) for the full privacy policy and what data flows occur (spoiler: only the model download and Tauri updater, both HTTPS to GitHub/HuggingFace).

## Contributing

We welcome issues and pull requests! See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, code style, and the PR process.

## Acknowledgements

- Default meta-prompt template adapted from [nidhinjs/prompt-master](https://github.com/nidhinjs/prompt-master) (MIT) — see [`assets/NOTICE`](assets/NOTICE).
- Inference powered by [llama.cpp](https://github.com/ggerganov/llama.cpp) (MIT).

## License

[MIT](LICENSE) © 2026 PrivatePrompter contributors.