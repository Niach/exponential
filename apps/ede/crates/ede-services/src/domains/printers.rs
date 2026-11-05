//! Printers through CUPS's `lpstat`: the queues, their state, the default.

use serde_json::{json, Value};

use super::command;
use crate::{unavailable, ActionSpec, Service};

pub struct Printers;

/// `lpstat -p` lines (`printer NAME is idle.  enabled since …`) and the
/// default from `lpstat -d` (`system default destination: NAME`).
pub fn parse(printers: &str, default: &str) -> Vec<Value> {
    let default_name = default.rsplit_once(": ").map(|(_, n)| n.trim().to_string()).unwrap_or_default();
    printers
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("printer ")?;
            let (name, state) = rest.split_once(" is ")?;
            let state = state.split('.').next().unwrap_or("").trim();
            Some(json!({"name": name, "state": state, "default": name == default_name}))
        })
        .collect()
}

impl Service for Printers {
    fn name(&self) -> &'static str {
        "printers"
    }
    fn description(&self) -> &'static str {
        "Print queues through CUPS."
    }

    fn snapshot(&self) -> Value {
        let Some(list) = command("lpstat", &["-p"]).or_else(|| command("lpstat", &["-a"]).map(|_| String::new())) else { return unavailable("CUPS (lpstat) is not available") };
        let default = command("lpstat", &["-d"]).unwrap_or_default();
        json!({"available": true, "printers": parse(&list, &default)})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![]
    }

    fn call(&self, action: &str, _args: &Value) -> Result<Value, String> {
        Err(format!("no action {action}"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lpstat_lines_parse() {
        let list = super::parse("printer HP_LaserJet is idle.  enabled since Mon\nprinter Office is disabled since Tue -\n", "system default destination: Office\n");
        assert_eq!(list.len(), 2);
        assert_eq!((list[0]["name"].as_str(), list[0]["state"].as_str(), list[0]["default"].as_bool()), (Some("HP_LaserJet"), Some("idle"), Some(false)));
        assert_eq!(list[1]["default"], true);
    }
}
