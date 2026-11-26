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

/// #EDE-245: `lpstat -o` lines (`Office-12 niach 1024 Wed 26 Nov 2026 …`),
/// the queued jobs Cinnamon's printers applet lists, each `{id, printer,
/// owner}` (the printer = the id up to its last `-`).
pub fn parse_jobs(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let id = words.next()?;
            let (printer, number) = id.rsplit_once('-')?;
            number.parse::<u32>().ok()?;
            Some(json!({"id": id, "printer": printer, "owner": words.next().unwrap_or_default()}))
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
    /// Queues come and go rarely; jobs while printing (#EDE-245).
    fn interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }

    fn snapshot(&self) -> Value {
        let Some(list) = command("lpstat", &["-p"]).or_else(|| command("lpstat", &["-a"]).map(|_| String::new())) else { return unavailable("CUPS (lpstat) is not available") };
        let default = command("lpstat", &["-d"]).unwrap_or_default();
        let jobs = parse_jobs(&command("lpstat", &["-o"]).unwrap_or_default());
        json!({"available": true, "printers": parse(&list, &default), "jobs": jobs})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("cancel", "Cancel one of the person's print jobs (its id from the snapshot).", json!({"job": {"type": "string"}}), &["job"], None),
            ActionSpec::new("cancel_all", "Cancel every print job the person may cancel.", json!({}), &[], None),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        match action {
            "cancel" => {
                let job = args["job"].as_str().filter(|j| !j.starts_with('-')).ok_or("job")?;
                command("cancel", &[job]).ok_or_else(|| format!("cancel {job} failed"))?;
                Ok(json!({"cancelled": job}))
            }
            "cancel_all" => {
                command("cancel", &["-a"]).ok_or("cancel -a failed")?;
                Ok(json!({"cancelled": "all"}))
            }
            other => Err(format!("no action {other}")),
        }
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

    #[test]
    fn lpstat_jobs_parse() {
        let jobs = super::parse_jobs("Office-12               niach          1024   Wed 26 Nov 2026 16:00:00 CET\nHP_Laser-Jet-3 root 2048 Wed\nnot a job\n");
        assert_eq!(jobs.len(), 2);
        assert_eq!((jobs[0]["id"].as_str(), jobs[0]["printer"].as_str(), jobs[0]["owner"].as_str()), (Some("Office-12"), Some("Office"), Some("niach")));
        assert_eq!(jobs[1]["printer"], "HP_Laser-Jet");
    }
}
