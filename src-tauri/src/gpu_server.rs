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

/// Status keywords Brev prints in the STATUS column.
const KNOWN_STATUS: [&str; 8] = [
    "RUNNING", "STOPPED", "STARTING", "STOPPING", "DELETING", "FAILED", "BUILDING", "PENDING",
];

/// Derive a short GPU label (e.g. "L4", "H100") from a Brev MACHINE string such
/// as `g2-standard-4:nvidia-l4:1`.
fn gpu_from_machine(machine: &str) -> String {
    for part in machine.split(':') {
        if let Some(rest) = part.strip_prefix("nvidia-") {
            return rest.to_uppercase();
        }
    }
    String::new()
}

/// Parse a single `brev ls` data row into a [`GpuServerStatus`]. Returns `None`
/// for the header line and for wrapped continuation lines (the GPU column often
/// wraps onto its own line in a narrow terminal).
fn parse_row(line: &str) -> Option<GpuServerStatus> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.len() < 2 {
        return None;
    }
    let name = toks[0];
    if name.eq_ignore_ascii_case("NAME") {
        return None;
    }
    let status = toks[1].to_uppercase();
    if !KNOWN_STATUS.contains(&status.as_str()) {
        return None;
    }
    let machine = toks
        .iter()
        .find(|t| t.contains(':'))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let mut gpu = gpu_from_machine(&machine);
    if gpu.is_empty() {
        if let Some(last) = toks.last() {
            if *last != name && !last.contains(':') && !last.eq_ignore_ascii_case(&status) {
                gpu = last.to_string();
            }
        }
    }
    Some(GpuServerStatus {
        installed: true,
        logged_in: true,
        instance: name.to_string(),
        status,
        gpu,
        machine,
        ssh_host: name.to_string(),
        message: String::new(),
    })
}

/// List ALL Brev instances in the user's org so the app can show every server,
/// not just the one configured in settings. On error (no CLI / not logged in)
/// returns a single placeholder status carrying the reason for the UI.
pub fn list() -> Vec<GpuServerStatus> {
    let placeholder = |status: &str, message: &str| GpuServerStatus {
        installed: brev_installed(),
        logged_in: false,
        instance: String::new(),
        status: status.to_string(),
        gpu: String::new(),
        machine: String::new(),
        ssh_host: String::new(),
        message: message.to_string(),
    };

    if !brev_installed() {
        return vec![placeholder(
            "NO_BREV",
            "La CLI de Brev no está instalada (~/.local/bin/brev).",
        )];
    }

    match run_brev(&["ls"]) {
        Ok(text) => {
            let servers: Vec<GpuServerStatus> = text.lines().filter_map(parse_row).collect();
            if servers.is_empty() {
                vec![GpuServerStatus {
                    installed: true,
                    logged_in: true,
                    instance: String::new(),
                    status: "EMPTY".into(),
                    gpu: String::new(),
                    machine: String::new(),
                    ssh_host: String::new(),
                    message: "No tienes instancias en tu organización de Brev.".into(),
                }]
            } else {
                servers
            }
        }
        Err(e) => {
            if looks_like_login_error(&e) {
                vec![placeholder(
                    "NOT_LOGGED_IN",
                    "No has iniciado sesión en Brev. Ejecuta 'brev login' en la terminal.",
                )]
            } else {
                let mut p = placeholder("ERROR", &e);
                p.logged_in = true;
                vec![p]
            }
        }
    }
}

