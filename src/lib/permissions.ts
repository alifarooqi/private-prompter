/**
 * Typed bindings for the `permissions` Tauri commands.
 *
 * The frontend should never `invoke()` raw strings — wrap each command so we
 * get type checking and a single place to change the wire format.
 */

import { invoke } from "@tauri-apps/api/core";

export interface PermissionStatus {
  granted: boolean;
  detail: string | null;
}

/**
 * Probe whether macOS Accessibility permission is granted to our process.
 *
 * Pass `prompt = true` on the first call to make macOS show the system
 * permission dialog. Subsequent calls pass `false` to avoid re-prompting.
 */
export async function checkAccessibilityPermission(
  prompt: boolean,
): Promise<PermissionStatus> {
  return invoke<PermissionStatus>("check_accessibility_permission", { prompt });
}

export async function openAccessibilitySettings(): Promise<void> {
  await invoke("open_accessibility_settings");
}