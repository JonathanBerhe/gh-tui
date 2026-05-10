//! `mmdc` (Mermaid CLI) detection + invocation.
//!
//! Detection runs synchronously at startup before the alt-screen is
//! entered so we don't shell out from inside the TUI. The result is a
//! plain `bool` stored in `AppCtx`; render workers consult it before
//! launching any process. When `mmdc` is missing we cache a `Failed`
//! state for every Mermaid block so the renderer falls through to the
//! existing placeholder text — no first-class error UI needed.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;

/// True when `mmdc --version` exits 0. Probed once at startup; the
/// result is read by the render worker before each invocation.
#[must_use]
pub fn detect_mmdc() -> bool {
    match Command::new("mmdc")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
    {
        Ok(s) if s.success() => {
            tracing::info!("mmdc detected; mermaid blocks will render as PNGs");
            true
        }
        Ok(s) => {
            tracing::warn!(status = ?s, "mmdc found but --version failed; falling back to placeholders");
            false
        }
        Err(e) => {
            tracing::info!(error = %e, "mmdc not on PATH; mermaid blocks will render as placeholders");
            false
        }
    }
}

/// Render a Mermaid source to PNG bytes by shelling out to `mmdc`.
/// Writes the source to a temp `.mmd` file, asks `mmdc` to write the
/// PNG to a sibling path, reads the PNG, then cleans up both.
///
/// Errors out if `mmdc` is missing, exits non-zero, or writes a file
/// that can't be read. Returns the raw PNG bytes ready for `image::load_from_memory`.
pub async fn render_to_png(hash: &str, source: &str) -> Result<Vec<u8>, String> {
    let dir = std::env::temp_dir();
    let input = dir.join(format!("gh-tui-mmd-{hash}.mmd"));
    let output = dir.join(format!("gh-tui-mmd-{hash}.png"));

    // Best-effort write; failures here are reported as render errors so
    // the cache slot transitions to Failed and the renderer falls back.
    {
        let mut f = tokio::fs::File::create(&input)
            .await
            .map_err(|e| format!("create temp .mmd: {e}"))?;
        f.write_all(source.as_bytes())
            .await
            .map_err(|e| format!("write temp .mmd: {e}"))?;
        f.flush().await.ok();
    }

    let status = TokioCommand::new("mmdc")
        .args(["-i"])
        .arg(&input)
        .args(["-o"])
        .arg(&output)
        // Transparent so the diagram blends with the terminal bg.
        .args(["-b", "transparent"])
        // Use the dark theme — pairs with the dark TUI palette better
        // than the default white.
        .args(["-t", "dark"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .await
        .map_err(|e| format!("spawn mmdc: {e}"))?;

    if !status.success() {
        cleanup(&input, &output);
        return Err(format!("mmdc exited {status}"));
    }

    let bytes = tokio::fs::read(&output)
        .await
        .map_err(|e| format!("read mmdc output: {e}"));
    cleanup(&input, &output);
    bytes
}

fn cleanup(input: &PathBuf, output: &PathBuf) {
    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(output);
}
