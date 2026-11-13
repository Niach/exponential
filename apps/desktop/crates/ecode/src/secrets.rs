//! `SecretSource`: a job's secrets go INTO its process environment and are
//! never handed back: the trait has no getter. Logs are redacted by the
//! same source (`remote.json`'s redaction rule).

use std::collections::BTreeMap;
use std::process::Command;

use crate::Result;

/// Injects named secrets into a process.
pub trait SecretSource: Send + Sync {
    /// Sets each secret this source has as an env var of the same name on
    /// `command`; returns the names it does not have.
    fn inject(&self, names: &[String], command: &mut Command) -> Result<Vec<String>>;
    /// `text` with the values of `names` replaced by the redaction mark.
    fn redact(&self, names: &[String], text: &str) -> String;
}

/// Secrets held in memory (a host that fetched them per job, a test).
/// `Debug` shows the names only.
#[derive(Default)]
pub struct MapSecrets {
    values: BTreeMap<String, String>,
}

impl MapSecrets {
    pub fn new<I: IntoIterator<Item = (String, String)>>(values: I) -> Self {
        Self { values: values.into_iter().collect() }
    }
}

impl std::fmt::Debug for MapSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.values.keys()).finish()
    }
}

impl SecretSource for MapSecrets {
    fn inject(&self, names: &[String], command: &mut Command) -> Result<Vec<String>> {
        let mut missing = vec![];
        for name in names {
            match self.values.get(name) {
                Some(value) => {
                    command.env(name, value);
                }
                None => missing.push(name.clone()),
            }
        }
        Ok(missing)
    }

    fn redact(&self, names: &[String], text: &str) -> String {
        let values: Vec<&str> = names.iter().filter_map(|n| self.values.get(n).map(String::as_str)).collect();
        ecode_core::redact(text, &values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_and_redacts_without_a_getter() {
        let secrets = MapSecrets::new([("TOKEN".to_string(), "s3cret".to_string())]);
        let mut command = Command::new("sh");
        let missing = secrets.inject(&["TOKEN".into(), "OTHER".into()], &mut command).unwrap();
        assert_eq!(missing, ["OTHER"]);
        assert!(command.get_envs().any(|(k, v)| k == "TOKEN" && v.is_some_and(|v| v == "s3cret")));
        assert_eq!(secrets.redact(&["TOKEN".into()], "token=s3cret"), "token=***");
        assert!(!format!("{secrets:?}").contains("s3cret"));
    }
}
