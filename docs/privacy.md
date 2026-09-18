# PrivatePrompter Privacy Policy

PrivatePrompter is privacy-first by design. **No analytics. No telemetry. No accounts.** This page describes exactly what data leaves your machine and when.

## Network calls

After the first-run model download, PrivatePrompter makes outbound requests **only** for:

- **Initial model download** — to `huggingface.co` over HTTPS. One-time per model.
- **Manual update check** — to `github.com` over HTTPS, only when you click *Check for Updates*.

There are no other network calls. There is no analytics endpoint. There is no telemetry beacon. There is no crash report. We have no servers.

## Local data

The application writes the following to `~/Library/Application Support/PrivatePrompter/`:

| Path | Contents |
| --- | --- |
| `models/<model-id>/<file>.gguf` | Downloaded LLM weights. |
| `templates/<name>.md` | User-edited prompt templates (if any). |
| `undo.json` | Last few rewrite entries with original + rewritten text. Cleared on app quit if you revoke storage access. |
| `logs/<today>.log` | Diagnostic logs (`tracing` output, no PII). |

You can delete the entire directory to wipe all of it.

## Permissions

PrivatePrompter requests the following macOS permissions:

- **Accessibility** — required to read the highlighted text and simulate `Cmd+C` / `Cmd+V`. macOS prompts for this the first time you trigger a rewrite.
- **Notifications** — optional. Used only for the "Undo available" toast after each rewrite. macOS prompts on first notification.
- **Apple Events** — implicit. Granted as a side-effect of Accessibility; lets us ask System Events for the frontmost app name.

These permissions never leave your device. macOS handles them.

## Highlighting what we *don't* do

We do not:

- Record keystrokes outside the explicit hotkey trigger.
- Transmit your prompts, contexts, or rewrites to any server.
- Phone home with usage statistics.
- Track you across applications.
- Use any cloud-based LLM provider.

## Reporting

If you observe PrivatePrompter making network calls outside of the two listed above, please open an issue on GitHub. That would be a serious bug.