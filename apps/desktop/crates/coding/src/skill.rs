//! EXP-763 — the run playbook: how Exponential works, loaded into EVERY
//! coding session's system prompt at start (chats, team agents,
//! children, every resume, the agent shell; claude and codex).
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
//! EXP-1292: the playbook is `runbook.md` (the session tree), and
//! the layers after it come from `sessions.brief`: the team prompt, then a
//! team agent's own instructions ([`system_append_layers`]).
//!
//! The document is markdown without front matter and names tools by their
//! exact `exponential_*` names so a deferred tool is one search away. The web
//! test `apps/web/src/lib/emcp/context-budget.test.ts` reads the same file
//! and fails when it names a tool the server does not register or grows
//! past [`RUN_SKILL_MAX_BYTES`].

/// The playbook FILE, verbatim: every section, the budget's subject. A run
/// gets [`playbook`]'s cut of it.
pub const RUN_SKILL: &str = include_str!("runbook.md");

/// The section only a DISPATCHER (a dispatcher root, a team agent) reads.
pub const DISPATCH_HEADING: &str = "## Agents";

/// The coding-work sections a dispatcher never reads: it hands that work
/// to children instead (S-3: `## Show` and `## Results` are everyone's).
pub const CODING_ONLY_HEADINGS: [&str; 2] = ["## Fan out", "## Pull requests"];

/// The playbook ONE run gets: a dispatcher's swaps the coding-work sections
/// ([`CODING_ONLY_HEADINGS`]) for [`DISPATCH_HEADING`]; every other run
/// reads everything but that section. Byte-exact cuts of [`RUN_SKILL`].
pub fn playbook(dispatch: bool) -> &'static str {
    static CODING: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    static DISPATCH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let cell = if dispatch { &DISPATCH } else { &CODING };
    cell.get_or_init(|| {
        playbook_sections(RUN_SKILL)
            .into_iter()
            .filter(|section| {
                let heading = section.lines().next().unwrap_or_default().trim_end();
                if dispatch {
                    !CODING_ONLY_HEADINGS.contains(&heading)
                } else {
                    heading != DISPATCH_HEADING
                }
            })
            .collect()
    })
}

/// `text` cut before every `## ` heading line; the chunks concatenate back
/// to `text`.
fn playbook_sections(text: &str) -> Vec<&str> {
    let mut starts = vec![0];
    starts.extend(text.match_indices("\n## ").map(|(at, _)| at + 1));
    starts.push(text.len());
    starts.windows(2).map(|pair| &text[pair[0]..pair[1]]).collect()
}

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
every session of the team and outranks the requester's additional instructions.";

/// EXP-1292 — the heading a team AGENT's own instructions ride under, after
/// the team prompt (`sessions.brief`'s `agentInstructions`, set only for a
/// session of a `kind = agent` tree).
pub const AGENT_INSTRUCTIONS_HEADING: &str = "# Agent instructions";

/// EXP-1292 — the framing sentence under that heading.
pub const AGENT_INSTRUCTIONS_LEAD: &str = "Written for this team agent in Exponential; it says what \
this agent is for and applies to every turn of its channel.";

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
/// shell carries neither. `dispatch` picks the dispatcher's [`playbook`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    /// #EXP-1514: where the launch's computer use happens
    /// ([`crate::computer_place`]); `None` = no computer use and nothing to
    /// say about it.
    pub computer_use: Option<ComputerUse>,
    /// The launch wired the `codemode` server.
    pub code_mode: bool,
    /// An agent's conversation session, the persistent kind
    /// ([`crate::launcher::dispatches`]); NEW-1: no chat root dispatches.
    pub dispatch: bool,
    /// #EXP-1961: a voice call's own session (`sessions.brief` `call`):
    /// [`VOICE_CALL_SECTION`].
    pub voice_call: bool,
}

impl Extras {
    /// The bare playbook (an agent shell, a test).
    pub const NONE: Extras = Extras { computer_use: None, code_mode: false, dispatch: false, voice_call: false };
}

/// #EXP-1514: the launch's computer use as its prompt tells it. The models
/// are `Settings::computer_use_model`, the alias the section tells the agent
/// to hand its screen-driving subagents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerUse {
    /// The person's screen: [`COMPUTER_USE_SECTION`].
    Screen(String),
    /// An agent space of the session's own: [`SPACE_COMPUTER_USE_SECTION`].
    Space(String),
    /// The device's switch is on and nothing can be driven (the state
    /// table's row 5): no `computer` server, ONE line saying why
    /// ([`computer_use_unavailable`]).
    Unavailable(String),
}

/// EXP-1196 — what a run is told when THIS device's Computer use switch is
/// on and the launcher wired the `computer` MCP server: the ladder (pixels
/// last), how positions work, and the limits the server enforces anyway.
/// Outside the playbook (`runbook.md`) on purpose: the playbook has no bytes to spare, and
/// most runs never get the server. `{model}` is where the subagent model
/// alias lands ([`computer_use_section`]).
pub const COMPUTER_USE_SECTION: &str = "# Computer use

This device lets you see and drive its desktop through the `computer` MCP server: `screenshot`, \
`list_windows`, `activate_window`, `left_click`, `double_click`, `right_click`, `drag`, `scroll`, \
`type`, `paste_text`, `copy_selection`, `read_clipboard`, `key`, `mouse_move`, `cursor_position`, `wait`; read the \
tool schemas. It is the person's \
own screen, pointer and keyboard, in the foreground: while you drive, their input is locked and a \
banner tells them so; their Esc aborts every call until they speak again.

- Pixels come last. Prefer the Exponential tools, the team's MCP servers, your shell, a CLI or \
an API; use the screen only when the task needs a desktop app or the person's logged-in browser.
- Say in your reply that you are taking the screen BEFORE the first action, and give it back \
(stop calling) as soon as the step is done; the person cannot use their computer meanwhile.
- Look, act, look: `screenshot`, then act on what it shows (every coordinate is in that \
image's pixels; `list_windows` + `activate_window` put the right window in front), then \
`screenshot` again to verify; an unverified effect is not success. A run works on one display at \
a time; `screenshot {display}` switches.
- Text: `paste_text` (one step, clipboard put back), not `type`, past a few characters. A secret \
is ALWAYS `paste_text` with `sensitive: true` from a private temp `file`; never `type` or echo \
it. Read text with `copy_selection`.
- Show the person a frame that matters: `exponential_show` with a `label`ed picture \
(through its returned upload line).
- Drive the screen from ONE subagent at a time on model `{model}` (the `Agent` tool's `model`, a \
fast one by design): there is one pointer and one keyboard, so screen work never runs in parallel; \
keep the plan and the final check in this conversation.
";

/// #EXP-1514 — [`COMPUTER_USE_SECTION`] for a run in an agent space of its
/// session's own (Exponential Desktop): nothing of the person's is taken,
/// the space outlives the run, and away from the machine the person sees it
/// only through the frames the agent shows. Same byte cap. `{model}` as
/// there ([`space_computer_use_section`]).
pub const SPACE_COMPUTER_USE_SECTION: &str = "# Computer use

You have an agent space on this device: a desktop of your own (one screen, its own pointer, \
keyboard and clipboard), apart from the person's, through the `computer` MCP server: \
`screenshot`, `list_windows`, `activate_window`, `left_click`, `double_click`, `right_click`, \
`drag`, `scroll`, `type`, `paste_text`, `copy_selection`, `read_clipboard`, `key`, `mouse_move`, \
`cursor_position`, `wait`, `launch_app`; read the tool schemas. Nothing of the person's is touched; \
the person at this computer can watch your space.

- Pixels come last. Prefer the Exponential tools, the team's MCP servers, your shell, a CLI or \
an API; use the space only when the task needs a desktop app.
- It started empty: start programs with `launch_app`. Apps start signed out. If the task needs \
the person's signed-in apps, say so: this device's Work screen decides, and a change applies \
from this session's next start.
- It stays as you left it for as long as this session lives and may hold what earlier turns \
left: look before you act (`screenshot` first). When a call says the space is new and empty, \
what ran in it is gone.
- Act on what a `screenshot` shows (every coordinate is in that image's pixels), then \
`screenshot` again to verify; an unverified effect is not success.
- Text: `paste_text`, not `type`, past a few characters; a secret is ALWAYS `paste_text` with \
`sensitive: true` from a private temp `file`.
- Show the person a labelled frame of each step that matters: `exponential_show` with a \
`label`ed picture (through its returned upload line). Away from the machine it is their only \
view of your space.
- Close apps and sign out of what you no longer need.
- Drive the space from ONE subagent at a time on model `{model}` (the `Agent` tool's `model`); \
keep the plan and the final check in this conversation.
";

/// #EXP-1514 (row 5): how [`computer_use_unavailable`]'s one line starts:
/// fixed text, so the context layout counts it as the launcher's.
pub const COMPUTER_USE_UNAVAILABLE_LEAD: &str = "Computer use is on for this device, but this run has no computer tools:";

/// #EXP-1961 (etalk voice-call.json `callSession`): what a CALL session is
/// told: it is spoken, so short sentences and no Markdown; small things it
/// does itself, bigger work goes to the chat. Outside the playbook like
/// computer use (the runbook is at its byte cap).
pub const VOICE_CALL_SECTION: &str = "# Voice call

You are on a phone call: every reply you write is read aloud to the person, and they answer by \
speaking. The brief's Call section names the chat you answer for.

- Speak: one to three short sentences, plain words. No Markdown, lists, tables, code, links or \
ids read out; say numbers and names the way a person would.
- Your first reply is your opening line: short, in your own words, fitting where the chat stands.
- Small things you do yourself while talking (a look-up, reading the chat, filing an issue); say \
what you are doing in a few words first.
- Ask before long work. Work that takes more than a minute goes to the chat as a message \
(`exponential_sessions_send` to the chat's session) and you say you handed it over.
- Messages from the chat reach you as they come; read them before you answer.
- When the call has ended you are asked for its summary: reply with it only, plain sentences.
";

/// [`COMPUTER_USE_SECTION`] with the subagent model alias filled in.
pub fn computer_use_section(model: &str) -> String {
    COMPUTER_USE_SECTION.replace("{model}", model)
}

/// [`SPACE_COMPUTER_USE_SECTION`] with the subagent model alias filled in.
pub fn space_computer_use_section(model: &str) -> String {
    SPACE_COMPUTER_USE_SECTION.replace("{model}", model)
}

/// #EXP-1514 (row 5): the ONE line a run gets when the device's switch is
/// on and nothing can be driven, with why (the machine's own words, kept on
/// one line), so the agent can tell the person.
pub fn computer_use_unavailable(reason: &str) -> String {
    let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{COMPUTER_USE_UNAVAILABLE_LEAD} {reason} If the task needs the screen, say so.")
}

/// The fixed text every run of a device gets before the team prompt: the
/// playbook, plus the computer-use section (or row 5's line) as the launch
/// carries it. S-3: code mode adds nothing here: the `codemode` server's
/// own instructions already say it (the old section duplicated them).
pub fn fixed_append(extras: &Extras) -> String {
    let mut text = playbook(extras.dispatch).to_string();
    match &extras.computer_use {
        Some(ComputerUse::Screen(model)) => {
            text = format!("{}\n\n{}", text.trim_end(), computer_use_section(model));
        }
        Some(ComputerUse::Space(model)) => {
            text = format!("{}\n\n{}", text.trim_end(), space_computer_use_section(model));
        }
        Some(ComputerUse::Unavailable(reason)) => {
            text = format!("{}\n\n{}\n", text.trim_end(), computer_use_unavailable(reason));
        }
        None => {}
    }
    if extras.voice_call {
        text = format!("{}\n\n{}", text.trim_end(), VOICE_CALL_SECTION);
    }
    text
}

/// [`system_append`] for a run that may carry the extra servers: the
/// playbook, their sections, then the team prompt LAST (it outranks all).
pub fn system_append_with(team_prompt: Option<&str>, extras: &Extras) -> String {
    system_append_layers(team_prompt, None, extras)
}

/// EXP-1292 — the whole append of ONE session launch, off its
/// `sessions.brief`: the playbook and the extras' sections, the team prompt
/// under [`TEAM_PROMPT_HEADING`], then a team agent's instructions under
/// [`AGENT_INSTRUCTIONS_HEADING`]. Blank layers are left out, so a chat
/// session of a team without a prompt still gets the playbook verbatim.
/// Both server layers count as `team` in the context layout
/// ([`crate::context_layout::team_bytes`]).
pub fn system_append_layers(
    team_prompt: Option<&str>,
    agent_instructions: Option<&str>,
    extras: &Extras,
) -> String {
    fn blank(text: Option<&str>) -> Option<&str> {
        text.map(str::trim).filter(|text| !text.is_empty())
    }
    let mut text = fixed_append(extras);
    for (heading, lead, body) in [
        (TEAM_PROMPT_HEADING, TEAM_PROMPT_LEAD, blank(team_prompt)),
        (AGENT_INSTRUCTIONS_HEADING, AGENT_INSTRUCTIONS_LEAD, blank(agent_instructions)),
    ] {
        if let Some(body) = body {
            text = format!("{}\n\n{heading}\n\n{lead}\n\n{body}\n", text.trim_end());
        }
    }
    text
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
            "runbook.md is {} bytes, cap {RUN_SKILL_MAX_BYTES}",
            RUN_SKILL.len()
        );
    }

    fn computer(model: &str) -> Extras {
        Extras { computer_use: Some(ComputerUse::Screen(model.to_string())), ..Extras::NONE }
    }

    /// EXP-1196: the computer-use section sits between the playbook and the
    /// team prompt, only when asked for, names the subagent model, and
    /// names no em dash either.
    #[test]
    fn the_computer_use_section_rides_between_the_playbook_and_the_team_prompt() {
        assert_eq!(system_append_with(None, &Extras::NONE), playbook(false));
        assert_eq!(system_append_with(Some("Rules"), &Extras::NONE), system_append(Some("Rules")));
        let alone = system_append_with(None, &computer("haiku"));
        assert!(alone.starts_with(playbook(false).trim_end()));
        assert!(alone.ends_with(&computer_use_section("haiku")));
        assert!(alone.contains("subagent at a time on model `haiku`"));
        assert!(!alone.contains("{model}"));
        let both = system_append_with(Some("Rules"), &computer("sonnet"));
        let section = both.find("# Computer use").unwrap();
        let team = both.find(TEAM_PROMPT_HEADING).unwrap();
        assert!(playbook(false).trim_end().len() < section && section < team);
        assert!(both.ends_with("\n\nRules\n"));
        assert!(both.contains("model `sonnet`"));
        assert!(!COMPUTER_USE_SECTION.contains('\u{2014}'));
        assert!(COMPUTER_USE_SECTION.len() < 1900, "{} bytes", COMPUTER_USE_SECTION.len());
    }

    /// #EXP-1514: a space run's section takes the person section's place:
    /// under the same cap, never "taking the screen", with what a space is
    /// (its own desktop, `launch_app`, signed out, watched, kept), the show
    /// duty and look before you act.
    #[test]
    fn a_space_run_gets_the_space_section_in_the_same_place() {
        let space = Extras { computer_use: Some(ComputerUse::Space("haiku".to_string())), ..Extras::NONE };
        let alone = system_append_with(None, &space);
        assert!(alone.starts_with(playbook(false).trim_end()));
        assert!(alone.ends_with(&space_computer_use_section("haiku")));
        assert!(!alone.contains(COMPUTER_USE_SECTION.lines().nth(2).unwrap()), "never the person's section");
        let both = system_append_with(Some("Rules"), &Extras { computer_use: Some(ComputerUse::Space("sonnet".to_string())), ..Extras::NONE });
        let section = both.find("# Computer use").unwrap();
        assert!(playbook(false).trim_end().len() < section && section < both.find(TEAM_PROMPT_HEADING).unwrap());
        assert!(both.contains("subagent at a time on model `sonnet`") && !both.contains("{model}"));
        assert!(SPACE_COMPUTER_USE_SECTION.len() < 1900, "{} bytes", SPACE_COMPUTER_USE_SECTION.len());
        assert!(!SPACE_COMPUTER_USE_SECTION.contains('\u{2014}'));
        let lower = SPACE_COMPUTER_USE_SECTION.to_lowercase();
        assert!(!lower.contains("taking the screen") && !lower.contains("locked") && !lower.contains("esc"), "nothing of the person's seat");
        for words in [
            "a desktop of your own",
            "`launch_app`",
            "Apps start signed out",
            "Show the person a labelled frame of each step that matters",
            "`exponential_show`",
            "their only view of your space",
            "the person at this computer can watch your space",
            "look before you act",
            "the space is new and empty",
            "a change applies from this session's next start",
            "Close apps and sign out",
        ] {
            assert!(SPACE_COMPUTER_USE_SECTION.contains(words), "the space section never says {words}");
        }
        assert!(mentioned_tools(SPACE_COMPUTER_USE_SECTION) == ["exponential_show"]);
    }

    /// #EXP-1514 (row 5): the switch is on and nothing can be driven: ONE
    /// line with the machine's reason where the section would be, and no
    /// section.
    #[test]
    fn nothing_to_drive_is_one_line_with_the_reason() {
        let none = Extras {
            computer_use: Some(ComputerUse::Unavailable("No display session (DISPLAY is not set).\n".to_string())),
            ..Extras::NONE
        };
        let text = system_append_with(Some("Rules"), &none);
        let line = computer_use_unavailable("No display session (DISPLAY is not set).");
        assert_eq!(line.lines().count(), 1);
        assert!(line.starts_with(COMPUTER_USE_UNAVAILABLE_LEAD) && line.contains("(DISPLAY is not set). If the task"), "{line}");
        assert!(text.starts_with(&format!("{}\n\n{line}\n\n{TEAM_PROMPT_HEADING}", playbook(false).trim_end())), "{text}");
        assert!(!text.contains("# Computer use"));
        let voice = system_append_with(None, &Extras { voice_call: true, ..none });
        assert!(voice.ends_with(VOICE_CALL_SECTION) && voice.contains(&format!("{line}\n\n# Voice call")));
    }

    /// S-3: code mode adds no section (the codemode server says it itself).
    #[test]
    fn code_mode_adds_no_section() {
        let code = Extras { code_mode: true, ..Extras::NONE };
        assert_eq!(system_append_with(None, &code), system_append_with(None, &Extras::NONE));
        assert!(!system_append_with(Some("Rules"), &code).contains("# Code mode"));
    }

    /// EXP-1025: no team prompt = the playbook, byte for byte.
    #[test]
    fn system_append_without_a_team_prompt_is_the_bare_playbook() {
        assert_eq!(system_append(None), playbook(false));
        assert_eq!(system_append(Some("")), playbook(false));
        assert_eq!(system_append(Some("  \n\t")), playbook(false));
    }

    /// EXP-1025: the team prompt follows the playbook under its own
    /// heading, verbatim, with one framing sentence between.
    #[test]
    fn system_append_places_the_team_prompt_after_the_playbook() {
        let text = system_append(Some("## Rules\n\n- Always run `bun test`.\n\n"));
        assert!(text.starts_with(playbook(false).trim_end()));
        let tail = &text[playbook(false).trim_end().len()..];
        assert_eq!(
            tail,
            "\n\n# Team instructions\n\nWritten by this team's owners in Exponential; it applies to \
every session of the team and outranks the requester's additional instructions.\n\n## Rules\n\n- Always \
run `bun test`.\n"
        );
        // The cap is the server's (`teams.update`), mirrored by the contract.
        assert_eq!(domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES, 12 * 1024);
    }

    /// EXP-1292: a team agent's instructions follow the team prompt under
    /// their own heading; either layer alone renders alone, and blank ones
    /// vanish.
    /// #EXP-1961: a call session gets the voice section after the playbook;
    /// every other launch never does.
    #[test]
    fn a_call_session_gets_the_voice_section() {
        let call = fixed_append(&Extras { voice_call: true, ..Extras::NONE });
        assert!(call.trim_end().ends_with(VOICE_CALL_SECTION.trim_end()));
        assert!(!fixed_append(&Extras::NONE).contains("# Voice call"));
        assert!(mentioned_tools(VOICE_CALL_SECTION).iter().all(|name| name.starts_with("exponential_sessions")));
    }

    #[test]
    fn agent_instructions_ride_after_the_team_prompt() {
        let extras = Extras::NONE;
        assert_eq!(system_append_layers(None, None, &extras), playbook(false));
        assert_eq!(system_append_layers(Some(" "), Some("\n"), &extras), playbook(false));
        assert_eq!(system_append_layers(Some("Rules"), None, &extras), system_append(Some("Rules")));
        let both = system_append_layers(Some("Rules"), Some("Watch the inbox."), &extras);
        let team = both.find(TEAM_PROMPT_HEADING).unwrap();
        let agent = both.find(AGENT_INSTRUCTIONS_HEADING).unwrap();
        assert!(playbook(false).trim_end().len() < team && team < agent);
        assert!(both.ends_with(&format!(
            "\n\n{AGENT_INSTRUCTIONS_HEADING}\n\n{AGENT_INSTRUCTIONS_LEAD}\n\nWatch the inbox.\n"
        )));
        let alone = system_append_layers(None, Some("Watch the inbox."), &Extras { code_mode: true, ..Extras::NONE });
        assert!(!alone.contains(TEAM_PROMPT_HEADING));
        assert!(!AGENT_INSTRUCTIONS_LEAD.contains('\u{2014}'));
    }

    #[test]
    fn the_playbook_teaches_the_session_tree_tools_by_exact_name() {
        // EXP-763: the whole point — the tools a run never discovered on its
        // own, spelled the way tool search finds them. EXP-1292: the v2
        // session tree (send, link, guide) replaced start/message/get.
        let tools = mentioned_tools(RUN_SKILL);
        for name in [
            "exponential_sessions",
            "exponential_sessions_send",
            "exponential_session_self",
            "exponential_issues",
            "exponential_people",
            "exponential_attachments",
            "exponential_pr",
            "exponential_events",
            "exponential_show",
        ] {
            assert!(tools.contains(&name), "playbook never names {name}");
        }
        for gone in [
            "exponential_sessions_start",
            "exponential_sessions_message",
            "exponential_threads_get",
            "exponential_sessions_guide",
            "exponential_guide_write",
            "exponential_sessions_get",
            "exponential_plan_propose",
            "exponential_report_bug",
        ] {
            assert!(!tools.contains(&gone), "playbook still names {gone}");
        }
        // The ref contract every client renders.
        assert!(RUN_SKILL.contains("`#IDENT`"));
    }

    #[test]
    fn the_playbook_carries_no_front_matter_or_dashes() {
        assert!(RUN_SKILL.starts_with("# "), "plain markdown, no YAML front matter");
        assert!(!RUN_SKILL.contains('\u{2014}'), "no em dashes");
        assert!(RUN_SKILL.contains("Never use `gh`"));
        assert!(!RUN_SKILL.contains("expu_"));
    }

    /// EXP-822: the playbook is the ONLY channel that reaches every resume,
    /// so the "your cwd is your subject, never hunt for the repository" rule
    /// lives here rather than in a prompt.
    #[test]
    fn the_playbook_pins_the_run_to_its_own_working_directory() {
        assert!(RUN_SKILL.contains("## Where you are"));
        assert!(RUN_SKILL.contains("Never look for the repository elsewhere on this machine"));
    }

    /// EXP-856: the one rule a run cannot learn from its own transcript.
    /// Messaging a live subagent resumes a SECOND copy of it from that
    /// transcript, and both copies then edit the same files.
    #[test]
    fn the_playbook_forbids_messaging_a_running_subagent() {
        assert!(RUN_SKILL.contains("a running subagent is never messaged"));
    }

    /// EXP-1292: children exist ONLY as playbook text (no device
    /// machinery), so the wording is the contract. EXP-1709: the merge train
    /// lands every PR (a stack with it), so `merge` only queues.
    #[test]
    fn the_playbook_teaches_children_and_the_merge_train() {
        assert!(RUN_SKILL.contains("## Fan out"));
        assert!(RUN_SKILL.contains("`exponential_sessions_send {parentId: <your session>, deviceId, text}`"));
        assert!(RUN_SKILL.contains("Caps: depth 3, 10 live sessions per tree"));
        assert!(RUN_SKILL.contains("the merge train lands it"));
        assert!(RUN_SKILL.contains("only queues it"));
        assert!(!RUN_SKILL.contains("`stack: true`"));
        assert!(RUN_SKILL.contains(DISPATCH_HEADING));
    }

    /// A dispatcher reads the Dispatch section and none of the coding-work
    /// ones; every other run reads everything else. Both are byte-exact
    /// cuts of the file, which holds each section once.
    #[test]
    fn the_playbook_cut_follows_dispatch() {
        let coding = playbook(false);
        let dispatch = playbook(true);
        assert!(!coding.contains(DISPATCH_HEADING));
        for heading in CODING_ONLY_HEADINGS {
            assert_eq!(RUN_SKILL.matches(heading).count(), 1, "{heading}");
            assert!(coding.contains(heading), "{heading}");
            assert!(!dispatch.contains(heading), "{heading}");
        }
        assert_eq!(RUN_SKILL.matches(DISPATCH_HEADING).count(), 1);
        for shared in ["# Working inside Exponential", "## Where you are", "## Scope", "## Talk", "## End"] {
            assert!(coding.contains(shared) && dispatch.contains(shared), "{shared}");
        }
        assert!(coding.len() < RUN_SKILL.len() && dispatch.len() < RUN_SKILL.len());
        // The dispatch rules, by their load-bearing words.
        let section = dispatch.split(DISPATCH_HEADING).nth(1).unwrap();
        for words in [
            "read-only",
            "`exponential_sessions_send {teamId, deviceId, issueIds?, text}`",
            "no `parentId`",
            "self-contained brief",
            "Reuse a recent session",
            "update your card",
            "Never wait for a child",
        ] {
            assert!(section.contains(words), "the Dispatch section never says {words}");
        }
        let fixed = fixed_append(&Extras { dispatch: true, ..Extras::NONE });
        assert_eq!(fixed, dispatch);
        assert!(system_append_layers(Some("Rules"), None, &Extras { dispatch: true, ..Extras::NONE })
            .starts_with(dispatch.trim_end()));
    }

    /// S-3: plan mode is gone; every run (dispatchers too) learns to SHOW:
    /// one card per turn, edited in place, ask and plan as cards.
    #[test]
    fn the_playbook_teaches_show() {
        for dispatch in [false, true] {
            let book = playbook(dispatch);
            let section = book.split("## Show\n").nth(1).unwrap().split("\n## ").next().unwrap();
            for words in ["Show, don't tell", "`exponential_show`", "`replace: true`", "`ask`", "`plan`", "upload:<label>", "end your turn after one", "never a `Card`", "text only = a plain message"] {
                assert!(section.contains(words), "the Show section never says {words}");
            }
        }
        assert!(!RUN_SKILL.contains("## Plan\n"));
        assert!(!RUN_SKILL.contains("plan_mode"));
    }

    #[test]
    fn mentioned_tools_dedups_and_skips_the_bare_prefix() {
        let names = mentioned_tools(
            "search exponential_* then exponential_issues_get, exponential_issues_get again (exponential_pr_open).",
        );
        assert_eq!(names, vec!["exponential_issues_get", "exponential_pr_open"]);
    }
}
