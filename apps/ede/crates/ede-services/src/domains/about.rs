//! About this machine: the host name, the distribution, the kernel, CPU,
//! memory, uptime, the Exponential Desktop version, and the system check
//! (#EDE-35): `exponential-compositor doctor --json` (GPU, displays, seat,
//! Xwayland, NVIDIA), run once in the background and again on `check`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

use serde_json::{json, Value};

use crate::{ActionSpec, Service};

pub struct About;

/// The last system check, and whether one is running.
static CHECKS: Mutex<Option<Value>> = Mutex::new(None);
static CHECKING: AtomicBool = AtomicBool::new(false);

/// What a level reads as in Settings.
fn status(level: &str) -> &'static str {
    match level {
        "ok" => "OK",
        "warn" => "Check",
        _ => "Fix needed",
    }
}

/// The doctor's rows with a `status` to show; `None` when the compositor's
/// doctor can't be run (not installed, unreadable output).
pub fn parse_checks(stdout: &str) -> Option<Value> {
    // The JSON is one line; anything else on stdout (a library's chatter) is skipped.
    let rows: Vec<Value> = stdout.lines().rev().find_map(|line| serde_json::from_str(line.trim()).ok())?;
    Some(Value::Array(
        rows.into_iter()
            .map(|mut row| {
                let level = row.get("level").and_then(Value::as_str).unwrap_or("fail").to_string();
                row["status"] = json!(status(&level));
                row
            })
            .collect(),
    ))
}

/// exponential-compositor next to us (the packages put both in /usr/bin), else PATH.
fn run_doctor() -> Option<Value> {
    let program = std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("exponential-compositor")))
        .filter(|p| p.exists())
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "exponential-compositor".into());
    // A failing check exits 1 with the same JSON: the output counts, not the status.
    let out = std::process::Command::new(program).args(["doctor", "--json"]).env_remove("RUST_LOG").output().ok()?;
    parse_checks(&String::from_utf8_lossy(&out.stdout))
}

fn store(checks: Option<Value>) -> Value {
    let checks = checks.unwrap_or_else(|| json!([]));
    *CHECKS.lock().unwrap_or_else(|p| p.into_inner()) = Some(checks.clone());
    checks
}

fn os_release() -> String {
    std::fs::read_to_string("/etc/os-release").ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))).unwrap_or_default()
}

fn mem_total_kb() -> u64 {
    std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| t.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse().ok())).unwrap_or(0)
}

fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("model name").map(|v| v.trim_start_matches([' ', '\t', ':']).trim().to_string()))).unwrap_or_default()
}

impl Service for About {
    fn name(&self) -> &'static str {
        "about"
    }
    fn description(&self) -> &'static str {
        "This machine: host, distribution, kernel, hardware."
    }

    fn snapshot(&self) -> Value {
        let hostname = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
        let kernel = super::command("uname", &["-r"]).map(|k| k.trim().to_string()).unwrap_or_default();
        let checks = CHECKS.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if checks.is_none() && !CHECKING.swap(true, Ordering::SeqCst) {
            // The first look runs the doctor off the poll thread (it probes the GPU).
            let _ = std::thread::Builder::new().name("exponential-services-doctor".into()).spawn(|| {
                store(run_doctor());
                CHECKING.store(false, Ordering::SeqCst);
            });
        }
        let uptime = std::fs::read_to_string("/proc/uptime").ok().and_then(|t| t.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())).unwrap_or(0.0) as u64;
        json!({
            "available": true,
            "hostname": hostname,
            "os": os_release(),
            "kernel": kernel,
            "cpu": cpu_model(),
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            "memoryMb": mem_total_kb() / 1024,
            "uptimeSeconds": uptime,
            "ede": env!("CARGO_PKG_VERSION"),
            "checks": checks.unwrap_or_else(|| json!([])),
            "checking": CHECKING.load(Ordering::SeqCst),
        })
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![ActionSpec::new("check", "Run the system check again (GPU, displays, seat, Xwayland, NVIDIA).", json!({}), &[], None)]
    }

    fn call(&self, action: &str, _args: &Value) -> Result<Value, String> {
        match action {
            "check" => {
                CHECKING.store(true, Ordering::SeqCst);
                let checks = store(run_doctor());
                CHECKING.store(false, Ordering::SeqCst);
                Ok(json!({"checks": checks}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_doctor_rows_get_a_status() {
        let rows = parse_checks(r#"[{"name":"NVIDIA","level":"fail","detail":"modeset is off"},{"name":"Xwayland","level":"ok","detail":"24.1"}]"#).unwrap();
        assert_eq!(rows[0]["status"], "Fix needed");
        assert_eq!(rows[1]["status"], "OK");
        assert_eq!(parse_checks("FAIL  NVIDIA x"), None);
        let noisy = "WARN smithay: Unable to become drm master\n[{\"name\":\"GPU\",\"level\":\"warn\",\"detail\":\"x\"}]\n";
        assert_eq!(parse_checks(noisy).unwrap()[0]["status"], "Check");
    }
}
