/**
 * Typed bindings for the hotkey Tauri commands + a browser-side key-code
 * mapper that turns a `KeyboardEvent.code` into the strings our Rust
 * `HotkeyConfig` accepts.
 */

import { invoke } from "@tauri-apps/api/core";

export interface HotkeyConfig {
  modifiers: string[];
  key: string;
}

export interface HotkeyInfo {
  config: HotkeyConfig;
  display: string;
}

export async function getHotkey(): Promise<HotkeyInfo> {
  return invoke<HotkeyInfo>("get_hotkey");
}

export async function setHotkey(config: HotkeyConfig): Promise<HotkeyInfo> {
  return invoke<HotkeyInfo>("set_hotkey", { config });
}

export async function resetHotkey(): Promise<HotkeyInfo> {
  return invoke<HotkeyInfo>("reset_hotkey");
}

/**
 * Convert a `KeyboardEvent.code` (e.g. "KeyR", "Digit5", "Space") into
 * the canonical key string Rust expects. Modifier keys ("ShiftLeft" etc.)
 * return null — they're tracked via the event's modifier flags, not the
 * key itself.
 */
export function codeToKey(code: string): string | null {
  // Letters: "KeyA".."KeyZ" → "A".."Z"
  if (/^Key[A-Z]$/.test(code)) {
    return code.slice(3);
  }
  // Digits: "Digit0".."Digit9" → "0".."9"
  if (/^Digit\d$/.test(code)) {
    return code.slice(5);
  }
  // Function keys: "F1".."F12"
  if (/^F(1[0-2]|[1-9])$/.test(code)) {
    return code;
  }
  // Whitespace / navigation / editing
  const fixed: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Delete: "Delete",
    Escape: "Escape",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    Insert: "Insert",
    Minus: "-",
    Equal: "=",
    BracketLeft: "[",
    BracketRight: "]",
    Backslash: "\\",
    Semicolon: ";",
    Quote: "'",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Backquote: "`",
  };
  return fixed[code] ?? null;
}

/**
 * Extract the modifier set from a keyboard event using the same string
 * names Rust expects: "super" / "alt" / "shift" / "ctrl". Order is
 * canonical (super, alt, shift, ctrl) so display output is stable.
 */
export function modifiersFromEvent(e: KeyboardEvent): string[] {
  const mods: string[] = [];
  if (e.metaKey) mods.push("super");
  if (e.altKey) mods.push("alt");
  if (e.shiftKey) mods.push("shift");
  if (e.ctrlKey) mods.push("ctrl");
  return mods;
}

/**
 * Human-readable rendering for any HotkeyConfig. Mirrors `display()` in
 * hotkey_config.rs so the UI can preview a combo without round-tripping.
 */
export function displayCombo(modifiers: string[], key: string): string {
  let s = "";
  for (const m of modifiers) {
    s +=
      m === "super" || m === "cmd" || m === "command"
        ? "⌘"
        : m === "alt" || m === "option"
          ? "⌥"
          : m === "shift"
            ? "⇧"
            : m === "ctrl" || m === "control"
              ? "⌃"
              : m;
  }
  return s + key.toUpperCase();
}