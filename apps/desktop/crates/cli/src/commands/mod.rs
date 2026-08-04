pub mod account;
pub mod code;
pub mod daemon;
pub mod doctor;
pub mod login;
pub mod run;
pub mod uninstall;
pub mod update;

use std::process::ExitCode;

pub type CommandResult = anyhow::Result<ExitCode>;

/// Tiny flag walker for the hand-rolled arg parsing: returns the value of
/// `--name <value>` (or `--name=value`) and strips it from `args`.
pub fn take_value(args: &mut Vec<String>, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    if let Some(index) = args.iter().position(|arg| arg.starts_with(&prefix)) {
        let value = args.remove(index)[prefix.len()..].to_string();
        return Some(value);
    }
    let index = args.iter().position(|arg| arg == name)?;
    if index + 1 >= args.len() {
        args.remove(index);
        return Some(String::new());
    }
    let value = args.remove(index + 1);
    args.remove(index);
    Some(value)
}

/// Returns whether the bare flag `--name` was present and strips it.
pub fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    match args.iter().position(|arg| arg == name) {
        Some(index) => {
            args.remove(index);
            true
        }
        None => false,
    }
}

/// Collect every `--name k=v` occurrence (repeatable), stripped from `args`.
pub fn take_values(args: &mut Vec<String>, name: &str) -> Vec<String> {
    let mut values = Vec::new();
    while let Some(value) = take_value(args, name) {
        if value.is_empty() {
            break;
        }
        values.push(value);
    }
    values
}

pub fn reject_unknown_flags(args: &[String]) -> anyhow::Result<()> {
    if let Some(flag) = args.iter().find(|arg| arg.starts_with('-')) {
        anyhow::bail!("unknown option `{flag}`");
    }
    Ok(())
}
