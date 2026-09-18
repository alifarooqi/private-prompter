/**
 * Typed bindings for the `model` and `inference` Tauri commands.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface ModelSummary {
  id: string;
  display_name: string;
  publisher: string;
  size_bytes: number;
  context_length: number;
  min_ram_bytes: number;
  downloaded: boolean;
}

export interface DownloadProgress {
  modelId: string;
  bytesDownloaded: number;
  totalBytes: number;
  state: "started" | "progress" | "verifying" | "completed" | "failed";
}

export interface ServerStatus {
  running: boolean;
  host: string | null;
  port: number | null;
  currentModelId: string | null;
  loading: boolean;
}

export interface InferenceHealth {
  status: "ok" | "no_model" | "loading";
  model_id: string | null;
}

export async function listModels(): Promise<ModelSummary[]> {
  return invoke<ModelSummary[]>("list_models");
}

export async function detectRam(): Promise<"less-than-8gb" | "between-8-and-16gb" | "between-16-and-32gb" | "at-least-32gb" | "unknown"> {
  return invoke<any>("detect_ram");
}

export async function recommendedModelId(): Promise<string> {
  return invoke<string>("recommended_model_id");
}

export async function startModelDownload(modelId: string): Promise<void> {
  return invoke<void>("start_model_download", { modelId });
}

export async function cancelModelDownload(): Promise<void> {
  return invoke<void>("cancel_model_download");
}

export async function startInference(): Promise<ServerStatus> {
  return invoke<ServerStatus>("start_inference");
}

export async function stopInference(): Promise<void> {
  return invoke<void>("stop_inference");
}

export async function inferenceStatus(): Promise<ServerStatus> {
  return invoke<ServerStatus>("inference_status");
}

/**
 * Quick liveness check against the llama-server `/health` endpoint.
 * Returns null when we don't have a base URL (server isn't started yet).
 */
export async function inferenceHealth(): Promise<InferenceHealth | null> {
  return invoke<InferenceHealth | null>("inference_health");
}

export function onDownloadProgress(
  cb: (p: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("model://download-progress", (e) => cb(e.payload));
}

export function formatBytes(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  const mb = bytes / 1024 ** 2;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${mb.toFixed(0)} MB`;
}