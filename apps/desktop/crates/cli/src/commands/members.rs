//! `exponential members`: the members whose private Assistant runs on this
//! server (#EXP-2161), each with its state and, while it waits for the
//! member's approval, the code to compare.

use std::process::ExitCode;

use super::{reject_unknown_flags, CommandResult};
use crate::members::MemberStatus;

pub fn run(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let rows = crate::members::status(&crate::context::data_dir());
    if rows.is_empty() {
        println!("No members' Assistants run here.");
        return Ok(ExitCode::SUCCESS);
    }
    for line in lines(&rows) {
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

/// A member's name is the server's: control characters never reach a terminal.
fn plain(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(60).collect()
}

fn lines(rows: &[MemberStatus]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            let name = if row.member.is_empty() { &row.space_id } else { &row.member };
            let code = row.code.as_deref().unwrap_or("-");
            format!("{:<24}  {:<10}  {}", plain(name), plain(&row.state), plain(code))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_per_member() {
        let rows = [
            MemberStatus { space_id: "s1".into(), member: "Ada\u{1b}[2J".into(), state: "waiting".into(), code: Some("ABCDE 12345".into()) },
            MemberStatus { space_id: "s2".into(), state: "stopped".into(), ..Default::default() },
        ];
        assert_eq!(lines(&rows), ["Ada[2J                    waiting     ABCDE 12345", "s2                        stopped     -"]);
    }
}
