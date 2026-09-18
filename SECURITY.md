# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| latest  | ✅        |

PrivatePrompter is pre-1.0. Only the latest tagged release receives updates. Older versions are unsupported.

## Reporting a Vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Email **INSERT-MAINTAINER-EMAIL** with:

- A clear description of the vulnerability and its impact.
- Reproduction instructions (PoC code, screenshots, or a minimal scenario).
- The affected version(s) and environment (macOS version, Apple Silicon vs Intel).

You should receive an acknowledgement within 72 hours. We will follow up with a timeline for a fix and (when applicable) coordinated disclosure.

## Threat Model

PrivatePrompter is a privacy-first tool. The threat model is:

- **In scope:** anything that could cause user text to leave the device, anything that allows privilege escalation from the app sandbox, anything that breaks the integrity of downloaded models or signed updates.
- **Out of scope:** bugs in the user's local LLM output quality, generic macOS permissions UX, third-party model GGUFs (we verify hashes against a manifest, but the model file itself is not authored by us).

## Network Endpoints

The application contacts the following endpoints. If you observe traffic to anything else, please report it.

- **HuggingFace** (`huggingface.co`) — initial model download only.
- **GitHub Releases** (`github.com`) — manual update checks + signed update payload delivery.

All other code paths must be offline. Verify by running the app with the network disabled after the initial model download.

## Disclosure Policy

We follow a coordinated disclosure model. Please give us a reasonable window to patch before public disclosure; we will credit reporters in release notes unless you prefer otherwise.