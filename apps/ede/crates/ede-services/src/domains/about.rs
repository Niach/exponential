//! About this machine: the host name, the distribution, the kernel, CPU,
//! memory, uptime, the eDE version.

use serde_json::{json, Value};

use crate::{ActionSpec, Service};

pub struct About;

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
        })
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![]
    }

    fn call(&self, action: &str, _args: &Value) -> Result<Value, String> {
        Err(format!("no action {action}"))
    }
}
