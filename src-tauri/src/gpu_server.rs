//! Control of the remote Brev GPU server (the box that trains the model) from
//! inside the app: read status, power on/off, and open a connection.
//!
//! Everything shells out to the locally-installed `brev` CLI. A GUI app launched
//! from Finder does not inherit `~/.local/bin` on its PATH, so we resolve the
//! binary path directly via [`crate::system::resolve_bin`].

use crate::models::GpuServerStatus;
use crate::system::resolve_bin;
use std::process::Command;

fn brev_bin() -> String {
    resolve_bin("brev", "BREV_PATH")
}

/// Run a `brev` subcommand, returning combined stdout/stderr on success or the
/// error text on failure.
fn run_brev(args: &[&str]) -> Result<String, String> {
    let out = Command::new(brev_bin())
        .args(args)
        .output()
        .map_err(|e| format!("No se pudo ejecutar brev: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() {
        Ok(format!("{stdout}\n{stderr}"))
    } else if stderr.trim().is_empty() {
        Err(stdout.trim().to_string())
    } else {
        Err(stderr.trim().to_string())
    }
}

fn brev_installed() -> bool {
    Command::new(brev_bin())
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn looks_like_login_error(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("log in")
        || e.contains("login")
        || e.contains("not authenticated")
        || e.contains("unauthorized")
        || e.contains("no organization")
        || e.contains("please authenticate")
}

/// Inspect the current status of `instance` by parsing `brev ls`.
pub fn status(instance: &str) -> GpuServerStatus {
    let mut s = GpuServerStatus {
        installed: brev_installed(),
        logged_in: false,
        instance: instance.to_string(),
        status: "UNKNOWN".into(),
        gpu: String::new(),
        machine: String::new(),
        ssh_host: instance.to_string(),
        message: String::new(),
    };

    if !s.installed {
        s.status = "NO_BREV".into();
        s.message = "La CLI de Brev no está instalada (~/.local/bin/brev).".into();
        return s;
    }

    match run_brev(&["ls"]) {
        Ok(text) => {
            s.logged_in = true;
            let mut found = false;
            for line in text.lines() {
                let toks: Vec<&str> = line.split_whitespace().collect();
                if toks.first().copied() == Some(instance) {
                    found = true;
                    if toks.len() >= 2 {
                        s.status = toks[1].to_uppercase();
                    }
                    // MACHINE column looks like "g2-standard-4:nvidia-l4:1".
                    if let Some(m) = toks.iter().find(|t| t.contains(':')) {
                        s.machine = m.to_string();
                    }
                    // GPU is the last column; guard against it being the name.
                    if let Some(last) = toks.last() {
                        if *last != instance && !last.contains(':') {
                            s.gpu = last.to_string();
                        }
                    }
                    break;
                }
            }
            if !found {
                s.status = "NOT_FOUND".into();
                s.message =
                    format!("La instancia '{instance}' no aparece en tu organización de Brev.");
            }
        }
        Err(e) => {
            if looks_like_login_error(&e) {
                s.status = "NOT_LOGGED_IN".into();
                s.message =
                    "No has iniciado sesión en Brev. Ejecuta 'brev login' en la terminal.".into();
            } else {
                s.status = "ERROR".into();
                s.message = e;
            }
        }
    }

    s
}

/// Power the instance on. Returns the refreshed status.
pub fn start(instance: &str) -> Result<GpuServerStatus, String> {
    run_brev(&["start", instance])?;
    Ok(status(instance))
}

/// Power the instance off. Returns the refreshed status.
pub fn stop(instance: &str) -> Result<GpuServerStatus, String> {
    run_brev(&["stop", instance])?;
    Ok(status(instance))
}
