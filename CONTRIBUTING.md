# Contributing to PrivatePrompter

Thanks for your interest! PrivatePrompter is a small project and your help — whether a typo fix, a new meta-prompt template, or a feature — is welcome.

## Code of Conduct

This project follows the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md). By participating you agree to its terms.

## Getting set up

### Prerequisites

- **macOS** (Apple Silicon or Intel) — PrivatePrompter is macOS-only for MVP.
- **Node.js 18+** and **npm** (or pnpm/yarn).
- **Rust** stable (install via [rustup](https://rustup.rs)).
- **Xcode Command Line Tools** — `xcode-select --install`.
- **CMake** and a C/C++ toolchain (needed by `llama.cpp` for sidecar builds).

### Bootstrap

```bash
git clone https://github.com/<owner>/private-prompter.git
cd private-prompter
npm install
npm run tauri dev
```

The first run will scaffold dependencies and boot a Tauri dev window. You will need to grant Accessibility permission in System Settings → Privacy & Security → Accessibility.

### Building the llama.cpp sidecar

The first time you need to test inference locally:

```bash
./scripts/build-sidecar.sh
```

This compiles `llama-server` for both `aarch64-apple-darwin` and `x86_64-apple-darwin` and drops the binaries into `src-tauri/binaries/`.

### Running tests

```bash
npm test                # frontend (vitest)
cd src-tauri && cargo test   # backend
```

## Project layout

See [`docs/architecture.md`](docs/architecture.md). Quick map:

- `src/` — React + TypeScript frontend (Vite).
- `src-tauri/` — Rust backend (Tauri commands, sidecar lifecycle, hotkey, clipboard, model manager).
- `assets/prompts/` — bundled meta-prompt templates.
- `assets/models.json` — registry of recommended GGUF models.
- `plan/` — design docs (the canonical roadmap).
- `scripts/` — build & release tooling.

## How to file issues

- **Bug reports** — use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.yml). Include macOS version, Apple Silicon vs Intel, model used, and steps to reproduce.
- **Feature requests** — use the [feature request template](.github/ISSUE_TEMPLATE/feature_request.yml). Describe the user, the problem, and the proposed solution.
- **Security issues** — please **do not** open a public issue. See [`SECURITY.md`](SECURITY.md).

## How to submit a Pull Request

1. Fork the repo and create a branch off `main`: `git checkout -b fix/short-description`.
2. Make your changes. Keep PRs focused on a single concern.
3. Run linting and tests locally:
    ```bash
    npm run lint
    npm test
    cd src-tauri && cargo fmt --all && cargo clippy -- -D warnings && cargo test
    ```
4. Use the [PR template](.github/PULL_REQUEST_TEMPLATE.md).
5. If your change touches user-visible behavior, please add or update tests where feasible.
6. CI must pass before merge. A maintainer will review.

## Code style

- **Rust:** `cargo fmt` + `cargo clippy -- -D warnings`. Module boundaries follow the layout in `src-tauri/src/` (commands, hotkey, clipboard, context, inference, model, prompt).
- **TypeScript:** ESLint + Prettier defaults. Tailwind utility classes for styling.
- **Commit messages:** Conventional commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`).

## Areas that need help

- **Meta-prompt templates** — the more useful templates bundled, the better the out-of-box UX.
- **Context detection heuristics** — improve Tier 2 routing for more apps/URLs.
- **Docs** — tutorials, screenshots, troubleshooting guides.
- **Localization** — UI strings + meta-prompt templates for non-English workflows.

## Releasing

Maintainers trigger releases by pushing a `vX.Y.Z` tag; the `release.yml` workflow builds, signs, notarizes, and publishes the `.dmg` to GitHub Releases. Do not cut releases without a maintainer.