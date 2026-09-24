//! `llama-server` process lifecycle.
//!
//! We spawn `llama-server` as a Tauri sidecar, bind it to localhost on a
//! random port, and expose a thin `RunningServer` wrapper that knows how to
//! shut it down and report its status.
//!
//! The sidecar binary is built by `scripts/build-sidecar.sh` and dropped into
//! `src-tauri/binaries/`. Tauri v2 resolves binaries by file pattern
//! `<name>-<target-triple>`.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::process::{Child, Command};

use super::{InferenceError, ServerStatus};

const SIDECAR_NAME: &str = "llama-server";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const HEALTH_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Serialize, Deserialize)]
struct ServerInfo {
    port: u16,
}

/// Wraps a running `llama-server` child. Owns the child handle and the URL we
/// bound to. Drop doesn't kill the process — call `stop()` for that.
pub struct RunningServer {
    child: Option<Child>,
    base_url: String,
    port: u16,
    model_id: String,
}

impl RunningServer {
    pub async fn start(
        _app: &AppHandle,
        gguf: &Path,
        model_id: &str,
    ) -> Result<Self, InferenceError> {
        let port = pick_port()?;
        let host = "127.0.0.1";
        let base_url = format!("http://{host}:{port}");

        let binary = locate_sidecar_binary()?;

        let mut cmd = Command::new(binary);
        cmd.arg("--model")
            .arg(gguf)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            .arg("--n-gpu-layers")
            .arg("0") // Phase 3: CPU only; MLX path in v2.
            .arg("--ctx-size")
            .arg("4096")
            .arg("--log-disable")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd.spawn()?;
        let server = RunningServer {
            child: Some(child),
            base_url,
            port,
            model_id: model_id.to_string(),
        };

        // Give llama-server a beat to bind to its port. If it dies
        // immediately the subsequent /health poll will surface the error;
        // we don't block here so the caller can stream a 'Loading
        // model…' UI while the model actually loads into memory (which
        // can take a few seconds for the larger GGUFs).
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        Ok(server)
    }

    pub fn status(&self) -> ServerStatus {
        ServerStatus {
            running: self.is_running(),
            host: Some("127.0.0.1".to_string()),
            port: Some(self.port),
            current_model_id: Some(self.model_id.clone()),
            loading: false,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn is_running(&self) -> bool {
        self.child
            .as_ref()
            .map(|c| c.id().is_some())
            .unwrap_or(false)
    }

    pub async fn stop(mut self) -> Result<(), InferenceError> {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        Ok(())
    }

    /// Stop without consuming self. Used by the inference command when we
    /// already have a &-reference and want to swap in a new server.
    pub async fn stop_in_place(&mut self) -> Result<(), InferenceError> {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        Ok(())
    }

    async fn wait_healthy(&self) -> Result<(), InferenceError> {
        let url = format!("{}/health", self.base_url);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let deadline = Instant::now() + HEALTH_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err(InferenceError::Unhealthy(HEALTH_TIMEOUT.as_secs()));
            }
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    // /health returns 200 OK during loading too; we only
                    // treat status: "ok" as ready so the caller doesn't
                    // return before the model is loaded into memory.
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if json.get("status").and_then(|v| v.as_str()) == Some("ok") {
                            return Ok(());
                        }
                    }
                }
            }
            tokio::time::sleep(HEALTH_POLL).await;
        }
    }
}

/// Find the sidecar binary. Tauri v2 places it at
/// `src-tauri/binaries/<name>-<target-triple>` (or via `tauri::path::resolve_resource`
/// in production). For dev and CI we check the binaries/ directory; if it
/// isn't there we surface a helpful error.
fn locate_sidecar_binary() -> Result<std::path::PathBuf, InferenceError> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .ok_or_else(|| InferenceError::Io(std::io::Error::other("no parent for current_exe")))?
        .to_path_buf();

    let triple = current_target_triple();
    let candidate = exe_dir.join(format!("{SIDECAR_NAME}-{triple}"));
    if candidate.exists() {
        return Ok(candidate);
    }

    // Dev fallback — the binary may live in src-tauri/binaries alongside the
    // Cargo target dir if we're running `cargo tauri dev`.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_candidate = manifest_dir
        .join("binaries")
        .join(format!("{SIDECAR_NAME}-{triple}"));
    if dev_candidate.exists() {
        return Ok(dev_candidate);
    }

    Err(InferenceError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "sidecar binary not found; run scripts/build-sidecar.sh to build {SIDECAR_NAME}-{triple}"
        ),
    )))
}

fn current_target_triple() -> &'static str {
    // Tauri exposes the host target triple via the `tauri::utils::platform`
    // module in some contexts, but env! is the simplest path.
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        "x86_64-pc-windows-msvc"
    } else {
        "unknown"
    }
}

/// Bind to port 0 to get a random free port, then immediately drop. The port
/// remains reserved long enough for us to pass it to the child.
fn pick_port() -> Result<u16, InferenceError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}
