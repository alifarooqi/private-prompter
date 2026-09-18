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

export async function checkAccessibilityPermission(): Promise<PermissionStatus> {
  return invoke<PermissionStatus>("check_accessibility_permission");
}

export async function openAccessibilitySettings(): Promise<void> {
  await invoke("open_accessibility_settings");
}