//! EXP-763 — the run playbook: how Exponential works, loaded into EVERY
//! coding session's system prompt at start (issue, batch, action, chat,
//! resume, agent shell; PTY and ACP; claude and codex).
//!
//! Why a system-prompt append and not a bigger launch prompt or a skill file:
//! the seed prompts stay lean (and a resume gets none), a skill directory
//! only lands a DESCRIPTION in context until the model decides to read the
//! body, and the MCP `instructions` string is capped at 2k chars. Each agent
//! has an additive text channel that is rebuilt on every launch, resume
//! included: claude `--append-system-prompt`, codex `developer_instructions`
//! (its own developer message, additive to AGENTS.md; `-c` on the PTY,
//! `developerInstructions` on `thread/start`/`thread/resume` in the
//! app-server). Both take TEXT, so nothing
//! new is written into the user's repo, scratch dir or agent home.
//!
//! The document is markdown without front matter and names tools by their
//! exact `exponential_*` names so a deferred tool is one search away. The web
//! test `apps/web/src/lib/mcp/context-budget.test.ts` reads the same file
//! and fails when it names a tool the server does not register or grows
//! past [`RUN_SKILL_MAX_BYTES`].

/// The playbook, verbatim.
pub const RUN_SKILL: &str = include_str!("skill.md");

/// Hard ceiling. Nothing truncates the text (it rides the agent's own
/// additive system-prompt channel), so the cap is a CONTEXT budget: the
/// playbook is prepended to every run, resume included, and has to stay
/// small enough to be worth that on every one of them. Mirrored by the web
/// gate in `context-budget.test.ts`.
pub const RUN_SKILL_MAX_BYTES: usize = 6 * 1024;

/// EXP-1025 — the heading the TEAM PROMPT rides under, after the playbook.
/// One level-1 heading like the playbook's own, so the two read as sibling
/// documents in the system prompt.
pub const TEAM_PROMPT_HEADING: &str = "# Team instructions";

/// EXP-1025 — the one framing sentence under that heading: who wrote the
/// text and where it ranks (above the requester's additional instructions,
/// which ride LAST in the seed prompt).
pub const TEAM_PROMPT_LEAD: &str = "Written by this team's owners in Exponential; it applies to \
every run of the team and outranks the requester's additional instructions.";

/// EXP-1025 — the text that goes onto the agent's additive system-prompt
/// channel for ONE run: the playbook, then the team prompt under
/// [`TEAM_PROMPT_HEADING`]. `None`/blank = the playbook alone, byte-identical
/// to what every run got before the team prompt existed, so a team without
/// one changes nothing.
///
/// Rebuilt on every start AND resume (the channel is), which is the whole
/// reason the team prompt lives here and not in the seed prompt: an edit in
/// Settings reaches the next resume of every existing run.
///
/// The text rides VERBATIM (trailing whitespace dropped): the server already
/// refused anything over `domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES`, and
/// a launcher that silently cut a prompt would hand the agent rules the
/// owner never wrote.
pub fn system_append(team_prompt: Option<&str>) -> String {
    system_append_with(team_prompt, &Extras::NONE)
}

/// EXP-1196/EXP-1236 — what ONE launch carries beyond the playbook: the
/// `computer` MCP server (with the model alias its screen-driving subagents
/// run on) and the `codemode` server. Each adds its own section; an agent
/// shell carries neither.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    /// `Some(model)` = the launch wired the `computer` server; `model` is
    /// `Settings::computer_use_model`, the alias the section tells the agent
    /// to hand its screen-driving subagents.
    pub computer_use: Option<String>,
    /// The launch wired the `codemode` server.
    pub code_mode: bool,
}

impl Extras {
    /// The bare playbook (an agent shell, a test).
    pub const NONE: Extras = Extras { computer_use: None, code_mode: false };
}

/// EXP-1196 — what a run is told when THIS device's Computer use switch is
/// on and the launcher wired the `computer` MCP server: the ladder (pixels
/// last), how positions work, and the limits the server enforces anyway.
/// Outside `skill.md` on purpose: the playbook has no bytes to spare, and
/// most runs never get the server. `{model}` is where the subagent model
/// alias lands ([`computer_use_section`]).
pub const COMPUTER_USE_SECTION: &str = "# Computer use

This device lets you see and drive its desktop through the `computer` MCP server (the cua \
driver): `list_apps`, `list_windows`, `get_window_state`, `click`, `type_text`, `press_key`, \
`hotkey`, `scroll`, `drag`, `invoke_menu`, `verify_state`, `get_desktop_state` and more; read the \
tool schemas. It is the person's own screen, pointer and keyboard.

- Pixels come last. Prefer the Exponential tools, the team's MCP servers, your shell, a CLI or \
an API; use the screen only when the task needs a desktop app or the person's logged-in browser.
- One exact target per action: `list_windows`, then `get_window_state({pid, window_id})` for \
its accessibility tree and screenshot, then act with `target: {kind: \"window\", pid, window_id}` \
and `delivery_mode: \"background\"`, through a fresh `element_token` or pixels of THAT window. \
Verify with `verify_state` or a fresh snapshot; an unverifiable effect is not success.
- Background keeps the person's pointer, focus and keyboard theirs. Only a `background_unavailable` \
refusal justifies `delivery_mode: \"foreground\"` or a desktop target (`get_desktop_state`, \
`{kind: \"desktop\", display_id: \"primary\"}`): say so in your reply first, because that takes \
over their pointer and keyboard.
- Show the person a frame that matters: `screenshot_out_file` on a state tool, then \
`exponential_sessions_show` with that `file`.
- Drive the screen from SUBAGENTS on model `{model}` (the `Agent` tool's `model`, a fast one by \
design): one window per subagent, several at once when the task has several, each verifying its \
own effects; keep the plan and the final check in this conversation. Background delivery drives \
windows in place, so parallel subagents need no second pointer.
";

/// [`COMPUTER_USE_SECTION`] with the subagent model alias filled in.
pub fn computer_use_section(model: &str) -> String {
    COMPUTER_USE_SECTION.replace("{model}", model)
}

/// EXP-1236 — what a run is told when the launcher wired the `codemode`
/// MCP server: one script instead of a call per item, parallel screen work,
/// and that only the script's output comes back.
pub const CODE_MODE_SECTION: &str = "# Code mode

The `codemode` MCP server runs JavaScript that calls this run's OTHER MCP tools: `exec({script})`, \
where `await tools.<server>.<tool>(args)` (or `tools.mcp__<server>__<tool>`) is any tool of \
`exponential`, `computer` and the team's HTTP servers; `Promise.all([...])` runs them at once (16 in \
flight), `sleep(ms)` waits, `console.log` lines and the `return` value come back as JSON (64 KB cap) \
and nothing else does. `describe({names})` gives input schemas; `ALL_TOOLS` lists the names inside a \
script.

- Use it when one step is many calls (every issue of a board, a dozen windows): one script, one \
result in your context, instead of a call per item. A single call stays a direct tool call.
- Computer use: one `tools.computer.*` chain per window inside `Promise.all` drives several windows \
at once; verify each effect in the script as you would a direct call, and print what you learned.
- Stdio team servers are direct-call only; a refusal rejects (`Promise.allSettled` tolerates it).
";

/// The fixed text every run of a device gets before the team prompt: the
/// playbook, plus [`COMPUTER_USE_SECTION`] and [`CODE_MODE_SECTION`] as the
/// launch carries them.
pub fn fixed_append(extras: &Extras) -> String {
    let mut text = RUN_SKILL.to_string();
    if let Some(model) = &extras.computer_use {
        text = format!("{}\n\n{}", text.trim_end(), computer_use_section(model));
    }
    if extras.code_mode {
        text = format!("{}\n\n{CODE_MODE_SECTION}", text.trim_end());
    }
    text
}

/// [`system_append`] for a run that may carry the extra servers: the
/// playbook, their sections, then the team prompt LAST (it outranks all).
pub fn system_append_with(team_prompt: Option<&str>, extras: &Extras) -> String {
    let fixed = fixed_append(extras);
    match team_prompt.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => format!(
            "{}\n\n{TEAM_PROMPT_HEADING}\n\n{TEAM_PROMPT_LEAD}\n\n{text}\n",
            fixed.trim_end()
        ),
        None => fixed,
    }
}

/// Every `exponential_*` name the playbook mentions, deduplicated, in order
/// of first appearance. The web drift gate checks the same set against the
/// registered tool surface.
pub fn mentioned_tools(text: &str) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(offset) = text[i..].find("exponential_") {
        let start = i + offset;
        let mut end = start + "exponential_".len();
        while end < bytes.len() && (bytes[end].is_ascii_lowercase() || bytes[end] == b'_') {
            end += 1;
        }
        let name = &text[start..end];
        // The bare prefix (`exponential_*`) is prose, not a tool.
        if name.len() > "exponential_".len() && !names.contains(&name) {
            names.push(name);
        }
        i = end;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_playbook_stays_inside_its_byte_budget() {
        assert!(!RUN_SKILL.trim().is_empty());
        assert!(
            RUN_SKILL.len() <= RUN_SKILL_MAX_BYTES,
            "skill.md is {} bytes, cap {RUN_SKILL_MAX_BYTES}",
            RUN_SKILL.len()
        );
    }

    fn computer(model: &str) -> Extras {
        Extras { computer_use: Some(model.to_string()), code_mode: false }
    }

    /// EXP-1196: the computer-use section sits between the playbook and the
    /// team prompt, only when asked for, names the subagent model, and
    /// names no em dash either.
    #[test]
    fn the_computer_use_section_rides_between_the_playbook_and_the_team_prompt() {
        assert_eq!(system_append_with(None, &Extras::NONE), RUN_SKILL);
        assert_eq!(system_append_with(Some("Rules"), &Extras::NONE), system_append(Some("Rules")));
        let alone = system_append_with(None, &computer("haiku"));
        assert!(alone.starts_with(RUN_SKILL.trim_end()));
        assert!(alone.ends_with(&computer_use_section("haiku")));
        assert!(alone.contains("SUBAGENTS on model `haiku`"));
        assert!(!alone.contains("{model}"));
        let both = system_append_with(Some("Rules"), &computer("sonnet"));
        let section = both.find("# Computer use").unwrap();
        let team = both.find(TEAM_PROMPT_HEADING).unwrap();
        assert!(RUN_SKILL.trim_end().len() < section && section < team);
        assert!(both.ends_with("\n\nRules\n"));
        assert!(both.contains("model `sonnet`"));
        assert!(!COMPUTER_USE_SECTION.contains('\u{2014}'));
        assert!(COMPUTER_USE_SECTION.len() < 1900, "{} bytes", COMPUTER_USE_SECTION.len());
    }

    /// EXP-1236: the code-mode section follows the computer-use one (when
    /// both are carried) and precedes the team prompt; alone it follows the
    /// playbook directly.
    #[test]
    fn the_code_mode_section_rides_after_computer_use_and_before_the_team_prompt() {
        let code = Extras { computer_use: None, code_mode: true };
        let alone = system_append_with(None, &code);
        assert!(alone.starts_with(RUN_SKILL.trim_end()));
        assert!(alone.ends_with(CODE_MODE_SECTION));
        assert!(!alone.contains("# Computer use"));
        let all = Extras { computer_use: Some("haiku".into()), code_mode: true };
        let everything = system_append_with(Some("Rules"), &all);
        let computer = everything.find("# Computer use").unwrap();
        let code_mode = everything.find("# Code mode").unwrap();
        let team = everything.find(TEAM_PROMPT_HEADING).unwrap();
        assert!(computer < code_mode && code_mode < team);
        assert!(everything.ends_with("\n\nRules\n"));
        assert!(!CODE_MODE_SECTION.contains('\u{2014}'));
        assert!(CODE_MODE_SECTION.len() < 1100, "{} bytes", CODE_MODE_SECTION.len());
        for name in ["`exec({script})`", "`describe({names})`", "Promise.all", "ALL_TOOLS", "tools.computer."] {
            assert!(CODE_MODE_SECTION.contains(name), "the section never says {name}");
        }
    }

    /// EXP-1025: no team prompt = the playbook, byte for byte.
    #[test]
    fn system_append_without_a_team_prompt_is_the_bare_playbook() {
        assert_eq!(system_append(None), RUN_SKILL);
        assert_eq!(system_append(Some("")), RUN_SKILL);
        assert_eq!(system_append(Some("  \n\t")), RUN_SKILL);
    }

    /// EXP-1025: the team prompt follows the playbook under its own
    /// heading, verbatim, with one framing sentence between.
    #[test]
    fn system_append_places_the_team_prompt_after_the_playbook() {
        let text = system_append(Some("## Rules\n\n- Always run `bun test`.\n\n"));
        assert!(text.starts_with(RUN_SKILL.trim_end()));
        let tail = &text[RUN_SKILL.trim_end().len()..];
        assert_eq!(
            tail,
            "\n\n# Team instructions\n\nWritten by this team's owners in Exponential; it applies to \
every run of the team and outranks the requester's additional instructions.\n\n## Rules\n\n- Always \
run `bun test`.\n"
        );
        // The cap is the server's (`teams.update`), mirrored by the contract.
        assert_eq!(domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES, 12 * 1024);
    }

    #[test]
    fn the_playbook_teaches_the_deferred_tools_by_exact_name() {
        // EXP-763: the whole point — the tools a run never discovered on its
        // own, spelled the way tool search finds them.
        let tools = mentioned_tools(RUN_SKILL);
        for name in [
            "exponential_sessions_start",
            "exponential_issues_create",
            "exponential_issue_relations_add",
            "exponential_report_bug",
            "exponential_devices_list",
            "exponential_sessions_message",
            "exponential_sessions_end",
            "exponential_sessions_ask_parent",
            // EXP-879/1251: the run publishes its Guide (pictures included).
            "exponential_sessions_guide",
            // EXP-1172: and shows each visible change while it works.
            "exponential_sessions_show",
        ] {
            assert!(tools.contains(&name), "playbook never names {name}");
        }
        // The ref contract every client renders.
        assert!(RUN_SKILL.contains("`#IDENT`"));
        // EXP-1188: sources as markdown links, other runs by their app url.
        assert!(RUN_SKILL.contains("markdown links"));
        assert!(RUN_SKILL.contains("`exponential_sessions_get`"));
    }

    #[test]
    fn the_playbook_carries_no_front_matter_or_dashes_and_teaches_parent_id() {
        assert!(RUN_SKILL.starts_with("# "), "plain markdown, no YAML front matter");
        assert!(!RUN_SKILL.contains('\u{2014}'), "no em dashes");
        // EXP-760: a sub-issue is one `issues_create` call with `parentId`;
        // every other relation goes through relations_add.
        assert!(RUN_SKILL.contains("`parentId`"));
        assert!(RUN_SKILL.contains("Never use `gh`"));
        assert!(!RUN_SKILL.contains("expu_"));
    }

    /// EXP-822: the playbook is the ONLY channel that reaches a run with no
    /// seed prompt (an attended chat started from the empty prompt box sends
    /// nothing) and every resume, so the "your cwd is your subject, never
    /// hunt for the repository" rule lives here rather than in a prompt.
    #[test]
    fn the_playbook_pins_the_run_to_its_own_working_directory() {
        assert!(RUN_SKILL.contains("## Your workspace"));
        assert!(RUN_SKILL.contains("Never look for the repository elsewhere on this machine"));
        assert!(RUN_SKILL.contains("ask which one"));
    }

    /// EXP-856: the one rule a run cannot learn from its own transcript.
    /// Messaging a live subagent resumes a SECOND copy of it from that
    /// transcript, and both copies then edit the same files. Mirrored by the
    /// web gate in `context-budget.test.ts`.
    #[test]
    fn the_playbook_forbids_messaging_a_running_subagent() {
        assert!(RUN_SKILL.contains("## Subagents"));
        assert!(RUN_SKILL.contains("never messaged"));
        assert!(RUN_SKILL.contains("SendMessage"));
    }

    /// SLOP-3: follow-up runs and root-first tree merges exist ONLY as
    /// playbook text (no server machinery), so the wording is the contract.
    #[test]
    fn the_playbook_teaches_follow_up_runs_and_root_first_merges() {
        assert!(RUN_SKILL.contains("## Follow-ups and follow-up runs"));
        assert!(RUN_SKILL.contains("base: \""));
        assert!(RUN_SKILL.contains("follow-up depth"));
        assert!(RUN_SKILL.contains("`no follow-up runs`"));
        assert!(RUN_SKILL.contains("root first"));
        assert!(RUN_SKILL.contains("Never merge a mid-tree PR alone"));
        assert!(RUN_SKILL.contains("`[Exponential child run ...]`"));
        assert!(!RUN_SKILL.contains("stackOnIssueId"));
        assert!(!RUN_SKILL.contains("'root'"));
        let tools = mentioned_tools(RUN_SKILL);
        for name in [
            "exponential_sessions_start",
            "exponential_sessions_message",
            "exponential_pr_open",
            "exponential_pr_merge",
            "exponential_pr_retarget",
            "exponential_sessions_ask_parent",
        ] {
            assert!(tools.contains(&name), "the playbook never names {name}");
        }
    }

    #[test]
    fn mentioned_tools_dedups_and_skips_the_bare_prefix() {
        let names = mentioned_tools(
            "search exponential_* then exponential_issues_get, exponential_issues_get again (exponential_pr_open).",
        );
        assert_eq!(names, vec!["exponential_issues_get", "exponential_pr_open"]);
    }
}
