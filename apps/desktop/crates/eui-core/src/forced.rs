//! #EXP-1781: FORCED interaction states, a HOST-ONLY option (never in the
//! catalog or the A2UI schema). A host pins `hover` / `pressed` / `focus` on
//! a node so a catalogue specimen shows what a pointer or a keyboard would
//! paint. PAINT ONLY: they feed the style resolution (recipe `when.state`, a
//! node's `:hover` / `:focus-visible` / `:pressed` blocks) and nothing else
//! (no event, no hover-opened overlay, no focus move, no accessibility
//! change), as a union over the host's real interaction states.
//! `fixtures/forced-states.json` is the spec; mirrors
//! `packages/exponential-ui/src/forced-states.ts`.

use indexmap::IndexMap;

/// The states a host may force.
pub const FORCED_STATE_NAMES: &[&str] = &["hover", "pressed", "focus"];

/// The order [`forced_states_of`] answers in.
const ORDER: &[&str] = &["hover", "pressed", "focus", "focus-visible"];

/// The normalized map (`focus` = a KEYBOARD focus: `focus` +
/// `focus-visible`) and the refused entries (`<id>:<state>`, input order).
pub fn normalize_forced_states<'a, I, S>(input: I) -> (IndexMap<String, Vec<String>>, Vec<String>)
where
    I: IntoIterator<Item = (&'a String, S)>,
    S: AsRef<[String]>,
{
    let mut forced = IndexMap::new();
    let mut dropped = Vec::new();
    for (id, states) in input {
        let mut out: Vec<String> = Vec::new();
        for s in states.as_ref() {
            if !FORCED_STATE_NAMES.contains(&s.as_str()) {
                dropped.push(format!("{id}:{s}"));
                continue;
            }
            let expanded: Vec<&str> = if s == "focus" { vec!["focus", "focus-visible"] } else { vec![s.as_str()] };
            for x in expanded {
                if !out.iter().any(|o| o == x) {
                    out.push(x.to_string());
                }
            }
        }
        if !out.is_empty() {
            forced.insert(id.clone(), out);
        }
    }
    (forced, dropped)
}

/// The forced states node or part `id` carries: the union of every key that
/// is `id` or a dotted prefix of it (`tabs` covers `tabs.tab.1`), in the
/// order hover, pressed, focus, focus-visible.
pub fn forced_states_of(forced: &IndexMap<String, Vec<String>>, id: &str) -> Vec<String> {
    let mut got: Vec<&str> = Vec::new();
    for (key, states) in forced {
        let covers = id == key || (id.len() > key.len() && id.starts_with(key.as_str()) && id.as_bytes()[key.len()] == b'.');
        if covers {
            got.extend(states.iter().map(String::as_str));
        }
    }
    ORDER.iter().filter(|s| got.contains(s)).map(|s| (*s).to_string()).collect()
}
