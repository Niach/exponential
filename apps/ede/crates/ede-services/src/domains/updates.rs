//! Updates through apt (Mint, Ubuntu): what is upgradable, a refresh and
//! an upgrade through `pkexec` (the sealed polkit prompt asks).

use ede_protocol::Capability;
use serde_json::{json, Value};

use super::command;
use crate::{unavailable, ActionSpec, Service};

pub struct Updates;

/// `apt list --upgradable` lines: `name/suite version arch [upgradable from: old]`.
pub fn parse_upgradable(text: &str) -> Vec<Value> {
    text.lines()
        .filter(|l| l.contains("[upgradable from:"))
        .filter_map(|l| {
            let (name, rest) = l.split_once('/')?;
            let mut parts = rest.split_whitespace();
            let suite = parts.next()?;
            let version = parts.next()?;
            let from = l.rsplit_once("from: ").map(|(_, f)| f.trim_end_matches(']').to_string()).unwrap_or_default();
            Some(json!({"name": name, "suite": suite, "version": version, "from": from}))
        })
        .collect()
}

impl Service for Updates {
    fn name(&self) -> &'static str {
        "updates"
    }
    fn description(&self) -> &'static str {
        "Package updates through apt."
    }

    fn snapshot(&self) -> Value {
        let Some(text) = command("apt", &["list", "--upgradable"]) else { return unavailable("apt is not available") };
        let packages = parse_upgradable(&text);
        let security = packages.iter().filter(|p| p["suite"].as_str().is_some_and(|s| s.contains("security"))).count();
        json!({"available": true, "count": packages.len(), "security": security, "packages": packages})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("refresh", "Refresh the package lists (asks for the password).", json!({}), &[], Some(Capability::System)),
            ActionSpec::new("upgrade", "Install every available update (asks for the password).", json!({}), &[], Some(Capability::System)),
        ]
    }

    fn call(&self, action: &str, _args: &Value) -> Result<Value, String> {
        let args: &[&str] = match action {
            "refresh" => &["apt-get", "update"],
            "upgrade" => &["apt-get", "-y", "upgrade"],
            other => return Err(format!("no action {other}")),
        };
        let status = std::process::Command::new("pkexec").args(args).env("DEBIAN_FRONTEND", "noninteractive").status().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(json!({"done": action}))
        } else {
            Err(format!("{action} ended with {status}"))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn apt_lines_parse() {
        let list = super::parse_upgradable("Listing...\nfirefox/noble-security 130.0+build2 amd64 [upgradable from: 129.0]\nvim/noble 2:9.1 amd64 [upgradable from: 2:9.0]\n");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["name"], "firefox");
        assert_eq!(list[0]["from"], "129.0");
        assert_eq!(list[1]["version"], "2:9.1");
    }
}
