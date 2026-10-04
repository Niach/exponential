//! EXP-879 — a coding run's PUBLISHED RESULTS: the pictures an agent files
//! while it works, read off `coding_sessions.results`.
//!
//! The column is a flat, ORDERED jsonb array of
//! `{topic, label, attachmentId, width, height}` — camelCase INSIDE the blob
//! (it is a document, not a row), `null`/`[]` = nothing published. This
//! module is the ONE reader on the desktop, the twin of the web
//! `lib/session-results.ts` and of the iOS/Android helpers: every client
//! parses the SAME way, so a blob one of them would drop is dropped
//! everywhere.
//!
//! Tolerance is the point. The blob is written by an agent through MCP and
//! hydrated out of SQLite (where a jsonb column arrives as a STRING holding
//! JSON, `hydrate::tolerant_opt_json`), so a reader that trusted its shape
//! would panic or blank the whole face over one bad entry. Instead: a string
//! blob is re-parsed, an entry missing its `topic`, `label` or
//! `attachmentId` is DROPPED, a width/height that is not a positive whole
//! number reads as unknown, unknown fields are ignored, and the list is
//! capped at [`MAX_SESSION_RESULTS`] — the same cap the server enforces, so
//! a row that somehow exceeds it renders the first 60 rather than nothing.

use serde_json::Value;

/// The server's cap on one run's results — mirrored here so an over-long
/// blob truncates rather than renders unbounded.
pub const MAX_SESSION_RESULTS: usize = 60;

/// The Results face's tile HEIGHT in px. Every tile in a topic's row is this
/// tall and takes its width from the probed aspect
/// ([`session_result_tile_width`]), so an iOS, an Android and a web shot of
/// the same screen line up on one baseline instead of stair-stepping.
pub const SESSION_RESULT_TILE_HEIGHT: f32 = 320.0;

/// EXP-1128: a picture whose probed width/height is UNDER this is TALL (a
/// full-page capture; a phone shot at ~0.46 never is). A tall picture takes
/// the 4:3 frame top-cropped with a Tall badge instead of rendering as a
/// sliver, and opens fit-to-width in a vertical scroll. Strict: exactly 1:3
/// is not tall. Fixture `session-results.json` `tiles` (×4).
pub const SESSION_RESULT_TALL_ASPECT: f32 = 1.0 / 3.0;

/// EXP-1172 — an `exponential_sessions_show` picture's tile in the run
/// transcript, one base height lower than a Results tile so a shot sits in
/// the conversation without swallowing it. Fixture `session-inline.json`.
pub const SESSION_INLINE_TILE_HEIGHT: f32 = 240.0;

/// EXP-1172 — the collapsed band a Results group folds its inline pictures
/// under (`Earlier · 3`).
pub const SESSION_RESULTS_EARLIER_LABEL: &str = "Earlier";

/// One published picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResultEntry {
    /// The heading the agent filed it under (the group band's text).
    pub topic: String,
    /// The one-line caption under the tile (usually the platform).
    pub label: String,
    /// The attachment row — the image is `/api/attachments/{id}`
    /// (member-gated, exactly like a comment attachment).
    pub attachment_id: String,
    /// The server's probe, when it got one: positive whole pixels or
    /// `None` (a tile with either side unknown falls back to 4:3).
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// EXP-1172 — filed by `exponential_sessions_show` while the run worked
    /// (true only for a JSON `true`).
    pub inline: bool,
    /// EXP-1172 — the show call's `text`, trimmed; `None` when blank.
    pub caption: Option<String>,
}

/// One topic's tiles, in the order they were published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResultGroup {
    pub topic: String,
    /// EXP-933 — the topic's GFM REPORT text (rendered ABOVE its pictures),
    /// `None` without one. A text-only topic is a group with no entries.
    pub text: Option<String>,
    pub entries: Vec<SessionResultEntry>,
    /// EXP-1172 — the topic's INLINE pictures, folded under the `Earlier`
    /// band (publish order); empty when the topic has a single picture.
    pub earlier: Vec<SessionResultEntry>,
    /// EXP-1154 — the repo paths the topic's report touched, off the SAME
    /// entry its text came from (trimmed, deduped first-seen, capped at
    /// [`SESSION_RESULT_FILES_MAX`]); empty without.
    pub files: Vec<String>,
}

/// EXP-1154 — the most paths one topic lists (fixture `files.maxFiles`).
pub const SESSION_RESULT_FILES_MAX: usize = 40;

/// EXP-1154 — the topic that leads the Guide unnumbered.
pub const SESSION_RESULTS_SUMMARY_TOPIC: &str = "Summary";

/// EXP-1154 — a text entry's `files`: strings only, trimmed, blanks and
/// duplicates dropped (first position kept), capped; a non-array = none.
fn result_files(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let Some(path) = item.as_str().map(str::trim).filter(|path| !path.is_empty()) else {
            continue;
        };
        if out.iter().any(|seen| seen == path) {
            continue;
        }
        out.push(path.to_string());
        if out.len() >= SESSION_RESULT_FILES_MAX {
            break;
        }
    }
    out
}

/// EXP-1154 — true for the Summary topic: trimmed, case-insensitive.
pub fn is_summary_topic(topic: &str) -> bool {
    topic.trim().eq_ignore_ascii_case(SESSION_RESULTS_SUMMARY_TOPIC)
}

/// EXP-1154 — one numbered Guide section: the group, its 1-based index and
/// the section count (the lead never counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideSection<'a, G> {
    pub group: &'a G,
    pub index: usize,
    pub total: usize,
}

/// EXP-1154 — the Results face as the GUIDE: the lead (the FIRST Summary
/// group wherever it sits, else `None`) and every other group as a numbered
/// section, in order. The twin of `@exp/ui` `guideSections`, fixture
/// `session-results.json` `guide.sections` (×4).
pub fn session_results_guide<'a, G>(
    groups: &'a [G],
    topic: impl Fn(&G) -> &str,
) -> (Option<&'a G>, Vec<GuideSection<'a, G>>) {
    let lead_index = groups.iter().position(|group| is_summary_topic(topic(group)));
    let rest: Vec<&G> = groups
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != lead_index)
        .map(|(_, group)| group)
        .collect();
    let total = rest.len();
    let sections = rest
        .into_iter()
        .enumerate()
        .map(|(index, group)| GuideSection { group, index: index + 1, total })
        .collect();
    (lead_index.map(|index| &groups[index]), sections)
}

/// EXP-1154 — `01 / 04`: both numbers two-digit zero-padded.
pub fn guide_section_caption(index: usize, total: usize) -> String {
    format!("{index:02} / {total:02}")
}

/// EXP-1154 — one Guide file row: the path and, when the loaded diff has the
/// file, its `(additions, deletions)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideFileRow {
    pub path: String,
    pub counts: Option<(u32, u32)>,
}

/// EXP-1154 — one row per path in order; counts from the diff file whose
/// path matches EXACTLY (`diff` = `(path, additions, deletions)`), else
/// `None` (an unknown path, or no diff loaded).
pub fn guide_file_rows(paths: &[String], diff: Option<&[(String, u32, u32)]>) -> Vec<GuideFileRow> {
    paths
        .iter()
        .map(|path| GuideFileRow {
            path: path.clone(),
            counts: diff.and_then(|files| {
                files
                    .iter()
                    .find(|(candidate, _, _)| candidate == path)
                    .map(|(_, additions, deletions)| (*additions, *deletions))
            }),
        })
        .collect()
}

/// EXP-1172 — a topic with more than one picture moves its inline ones into
/// `earlier`, so the final report leads; a topic's only picture stays.
fn fold_inline(mut groups: Vec<SessionResultGroup>) -> Vec<SessionResultGroup> {
    for group in &mut groups {
        if group.entries.len() < 2 {
            continue;
        }
        let (earlier, entries) = std::mem::take(&mut group.entries)
            .into_iter()
            .partition(|entry| entry.inline);
        group.earlier = earlier;
        group.entries = entries;
    }
    groups
}

/// The blob's array elements, whether it arrived structured or as the TEXT
/// SQLite hands a jsonb column over as. Anything else = nothing.
fn blob_items(raw: Option<&Value>) -> Vec<Value> {
    match raw {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::String(text)) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Array(items)) => items,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// EXP-933 — the Results face as a REPORT: pictures AND each topic's text
/// (`{topic, label: null, attachmentId: null, text}`), grouped in FIRST-SEEN
/// topic order whichever kind opened the topic. A topic's text is its FIRST
/// non-blank text entry, trimmed; an element usable as a picture is a picture
/// (its `text` ignored); pictures keep the [`MAX_SESSION_RESULTS`] cap. The
/// twin of `@exp/ui` `parseSessionResultGroups`, locked by
/// `packages/domain-contract/fixtures/session-results.json` (×4).
pub fn parse_session_result_groups(raw: Option<&Value>) -> Vec<SessionResultGroup> {
    let mut groups: Vec<SessionResultGroup> = Vec::new();
    fn open<'a>(groups: &'a mut Vec<SessionResultGroup>, topic: &str) -> &'a mut SessionResultGroup {
        let index = match groups.iter().position(|group| group.topic == topic) {
            Some(index) => index,
            None => {
                groups.push(SessionResultGroup {
                    topic: topic.to_string(),
                    text: None,
                    entries: Vec::new(),
                    earlier: Vec::new(),
                    files: Vec::new(),
                });
                groups.len() - 1
            }
        };
        &mut groups[index]
    }
    let mut pictures = 0usize;
    for item in blob_items(raw) {
        if let Some(entry) = entry_from(&item) {
            if pictures >= MAX_SESSION_RESULTS {
                continue;
            }
            pictures += 1;
            let topic = entry.topic.clone();
            open(&mut groups, &topic).entries.push(entry);
            continue;
        }
        let Some(object) = item.as_object() else {
            continue;
        };
        let field = |key: &str| -> Option<String> {
            let value = object.get(key)?.as_str()?.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        let (Some(topic), Some(body)) = (field("topic"), field("text")) else {
            continue;
        };
        let group = open(&mut groups, &topic);
        if group.text.is_none() {
            group.text = Some(body);
            group.files = result_files(object.get("files"));
        }
    }
    fold_inline(groups)
}

/// EXP-933 — true when the blob has anything for the Results face to show
/// (a picture OR a topic's text).
pub fn has_session_results(raw: Option<&Value>) -> bool {
    !parse_session_result_groups(raw).is_empty()
}

/// Read `coding_sessions.results` — see the module docs for the tolerance
/// rules. Anything that is not an array (or a JSON string holding one)
/// yields an empty list.
pub fn parse_session_results(raw: Option<&Value>) -> Vec<SessionResultEntry> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    // The store hands a jsonb column over as TEXT; a wire-delivered array
    // arrives structured. Both are the same list.
    let reparsed;
    let items = match raw {
        Value::Array(items) => items,
        Value::String(text) => {
            reparsed = serde_json::from_str::<Value>(text).ok();
            match reparsed.as_ref() {
                Some(Value::Array(items)) => items,
                _ => return Vec::new(),
            }
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(entry_from)
        .take(MAX_SESSION_RESULTS)
        .collect()
}

/// Group parsed entries by topic in FIRST-SEEN order — the page's reading
/// order is the agent's publishing order, never an alphabetical re-sort.
pub fn group_session_results(entries: &[SessionResultEntry]) -> Vec<SessionResultGroup> {
    let mut groups: Vec<SessionResultGroup> = Vec::new();
    for entry in entries {
        match groups.iter_mut().find(|group| group.topic == entry.topic) {
            Some(group) => group.entries.push(entry.clone()),
            None => groups.push(SessionResultGroup {
                topic: entry.topic.clone(),
                text: None,
                entries: vec![entry.clone()],
                earlier: Vec::new(),
                files: Vec::new(),
            }),
        }
    }
    fold_inline(groups)
}

/// Every picture of a set of groups, in order — what the tile sizing reads
/// (EXP-1172: the folded `earlier` ones too, so expanding the band never
/// resizes the page).
pub fn session_result_pictures(groups: &[SessionResultGroup]) -> Vec<SessionResultEntry> {
    groups
        .iter()
        .flat_map(|group| group.entries.iter().chain(group.earlier.iter()).cloned())
        .collect()
}

/// EXP-1172 — the picture an `exponential_sessions_show` call filed, by the
/// attachment id its answer carried (`preview.id`); `None` while the upload
/// is still in flight, once it was removed, or for a blank id.
pub fn session_result_picture(raw: Option<&Value>, attachment_id: &str) -> Option<SessionResultEntry> {
    let id = attachment_id.trim();
    if id.is_empty() {
        return None;
    }
    parse_session_results(raw)
        .into_iter()
        .find(|entry| entry.attachment_id == id)
}

/// EXP-1172 — the line under a transcript tile: the show call's caption,
/// else the picture's label.
pub fn session_result_tile_caption(entry: &SessionResultEntry) -> &str {
    entry.caption.as_deref().unwrap_or(&entry.label)
}

/// EXP-1128: true when the probed aspect is under
/// [`SESSION_RESULT_TALL_ASPECT`]; an unmeasured picture is never tall.
pub fn session_result_is_tall(entry: &SessionResultEntry) -> bool {
    match (entry.width, entry.height) {
        (Some(width), Some(tall)) if width > 0 && tall > 0 => {
            (width as f32 / tall as f32) < SESSION_RESULT_TALL_ASPECT
        }
        _ => false,
    }
}

/// The tile's width at `height`: the probed aspect, or 4:3 when either side
/// is unknown. A TALL picture (EXP-1128) takes the 4:3 frame too — the tile
/// shows its top, never a sliver. Rounded — a fractional pixel width leaves
/// a hairline seam between a tile and its border.
pub fn session_result_tile_width(entry: &SessionResultEntry, height: f32) -> f32 {
    let aspect = match (entry.width, entry.height) {
        (Some(width), Some(tall)) if width > 0 && tall > 0 && !session_result_is_tall(entry) => {
            width as f32 / tall as f32
        }
        _ => 4.0 / 3.0,
    };
    (height * aspect).round()
}

/// The height EVERY tile on the page takes given the width the tiles row
/// actually has: [`SESSION_RESULT_TILE_HEIGHT`] unless the WIDEST tile would
/// overflow that width, in which case the whole page scales down by the ONE
/// factor that makes it fit.
///
/// Why: a landscape shot is 480 px wide at the base height, wider than a
/// phone's content column on iOS and Android and wider than a narrow desktop
/// window's work column — it would clip at the right edge. Scaling each tile
/// on its own would break the shared baseline the equal-height rule exists
/// for (a web and an iOS shot of the same screen must line up), so the
/// factor is computed once for the page and applied to every tile. The rule
/// is shared ×4: same inputs, same height, on every client.
pub fn session_result_tile_height_fitting(
    entries: &[SessionResultEntry],
    available_width: f32,
) -> f32 {
    session_result_tile_height_fitting_from(entries, available_width, SESSION_RESULT_TILE_HEIGHT)
}

/// [`session_result_tile_height_fitting`] from another `base` — EXP-1172: the
/// transcript's inline tile fits from [`SESSION_INLINE_TILE_HEIGHT`].
pub fn session_result_tile_height_fitting_from(
    entries: &[SessionResultEntry],
    available_width: f32,
    base: f32,
) -> f32 {
    // An unmeasured or nonsense width is not a constraint — never shrink the
    // page because the layout has not reported yet.
    if !available_width.is_finite() || available_width <= 0.0 {
        return base;
    }
    let widest = entries
        .iter()
        .map(|entry| session_result_tile_width(entry, base))
        .fold(0.0_f32, f32::max);
    if widest <= available_width {
        return base;
    }
    // Floor, so the scaled tile lands INSIDE the width rather than a rounding
    // hair outside it; at least one pixel, so a tile is never zero-sized.
    (base * available_width / widest).floor().max(1.0)
}

/// One array element → an entry, or `None` when it is not usable.
fn entry_from(value: &Value) -> Option<SessionResultEntry> {
    let object = value.as_object()?;
    let text = |key: &str| -> Option<String> {
        let value = object.get(key)?.as_str()?.trim();
        (!value.is_empty()).then(|| value.to_string())
    };
    Some(SessionResultEntry {
        topic: text("topic")?,
        label: text("label")?,
        attachment_id: text("attachmentId")?,
        width: dimension(object.get("width")),
        height: dimension(object.get("height")),
        inline: object.get("inline").and_then(Value::as_bool) == Some(true),
        caption: text("caption"),
    })
}

/// A probed pixel side: a POSITIVE WHOLE number, or unknown. A float with a
/// fraction, zero, a negative and a non-numeric all read as unknown rather
/// than as a bogus aspect.
fn dimension(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        // Electric ships integer columns as text; a blob written through a
        // sloppy client can do the same inside the document.
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !number.is_finite() || number <= 0.0 || number.fract() != 0.0 {
        return None;
    }
    Some(number as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(topic: &str, label: &str, id: &str) -> SessionResultEntry {
        SessionResultEntry {
            topic: topic.to_string(),
            label: label.to_string(),
            attachment_id: id.to_string(),
            width: None,
            height: None,
            inline: false,
            caption: None,
        }
    }

    fn inline_json(entry: &SessionResultEntry) -> Value {
        serde_json::json!({
            "label": entry.label,
            "attachmentId": entry.attachment_id,
            "inline": entry.inline,
            "caption": entry.caption,
        })
    }

    /// EXP-1172 — the shared `session-inline.json` fixture ×4: the Results
    /// fold (`groups`) and the transcript tile's lookup (`lookup`).
    #[test]
    fn session_inline_matches_the_shared_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-inline.json"
        ))
        .unwrap();
        assert_eq!(fixture["inlineTileHeight"].as_f64().unwrap() as f32, SESSION_INLINE_TILE_HEIGHT);
        assert_eq!(fixture["earlierLabel"].as_str().unwrap(), SESSION_RESULTS_EARLIER_LABEL);
        let cases = fixture["groups"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let groups = parse_session_result_groups(Some(&case["raw"]));
            let actual: Vec<Value> = groups
                .iter()
                .map(|group| {
                    serde_json::json!({
                        "topic": group.topic,
                        "text": group.text,
                        "entries": group.entries.iter().map(inline_json).collect::<Vec<_>>(),
                        "earlier": group.earlier.iter().map(inline_json).collect::<Vec<_>>(),
                    })
                })
                .collect();
            assert_eq!(Value::Array(actual), case["expected"], "fixture case: {name}");
            // The pictures-only grouping folds the same way.
            let pictures = group_session_results(&parse_session_results(Some(&case["raw"])));
            let folded: Vec<_> = groups
                .iter()
                .filter(|group| !group.entries.is_empty() || !group.earlier.is_empty())
                .map(|group| (group.entries.clone(), group.earlier.clone()))
                .collect();
            assert_eq!(
                pictures
                    .iter()
                    .map(|group| (group.entries.clone(), group.earlier.clone()))
                    .collect::<Vec<_>>(),
                folded,
                "fixture case: {name}"
            );
        }
        let lookups = fixture["lookup"].as_array().unwrap();
        assert!(!lookups.is_empty());
        for case in lookups {
            let name = case["name"].as_str().unwrap();
            let found = session_result_picture(
                Some(&case["raw"]),
                case["attachmentId"].as_str().unwrap(),
            );
            let actual = found.map_or(Value::Null, |entry| {
                serde_json::json!({
                    "label": entry.label,
                    "inline": entry.inline,
                    "caption": entry.caption,
                    "tileCaption": session_result_tile_caption(&entry),
                })
            });
            assert_eq!(actual, case["expected"], "fixture case: {name}");
        }
    }

    /// EXP-933 — the shared fixture's `groups` cases, byte for byte the
    /// web/iOS/Android ones.
    #[test]
    fn parse_session_result_groups_matches_the_shared_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let cases = fixture["groups"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let raw = &case["raw"];
            let groups = parse_session_result_groups(Some(raw));
            let actual: Vec<Value> = groups
                .iter()
                .map(|group| {
                    serde_json::json!({
                        "topic": group.topic,
                        "text": group.text,
                        "entries": group.entries.iter().map(|entry| serde_json::json!({
                            "label": entry.label,
                            "attachmentId": entry.attachment_id,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            assert_eq!(Value::Array(actual), case["expected"], "fixture case: {name}");
            assert_eq!(
                has_session_results(Some(raw)),
                !groups.is_empty(),
                "fixture case: {name}"
            );
        }
        assert!(!has_session_results(None));
    }

    /// EXP-1154 — the fixture's `files` block: a group's files ride the
    /// entry its text came from.
    #[test]
    fn parse_session_result_groups_carries_files_off_the_winning_text() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["files"]["maxFiles"].as_u64().unwrap() as usize,
            SESSION_RESULT_FILES_MAX
        );
        let cases = fixture["files"]["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let actual: Vec<Value> = parse_session_result_groups(Some(&case["raw"]))
                .iter()
                .map(|group| serde_json::json!({"topic": group.topic, "files": group.files}))
                .collect();
            assert_eq!(Value::Array(actual), case["expected"], "fixture case: {name}");
        }
    }

    /// EXP-1154 — the over-cap rule: the 41st distinct path is dropped.
    #[test]
    fn parse_session_result_groups_caps_files() {
        let files: Vec<String> = (0..50).map(|index| format!("f{index}.ts")).collect();
        let raw = serde_json::json!([{"topic": "t", "text": "x", "files": files}]);
        let groups = parse_session_result_groups(Some(&raw));
        assert_eq!(groups[0].files.len(), SESSION_RESULT_FILES_MAX);
        assert_eq!(groups[0].files[39], "f39.ts");
    }

    /// EXP-1154 — the fixture's `guide.sections`: Summary leads wherever it
    /// sits; every other topic is numbered.
    #[test]
    fn session_results_guide_matches_the_shared_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let cases = fixture["guide"]["sections"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let topics: Vec<String> = case["topics"]
                .as_array()
                .unwrap()
                .iter()
                .map(|topic| topic.as_str().unwrap().to_string())
                .collect();
            let (lead, sections) = session_results_guide(&topics, |topic| topic.as_str());
            assert_eq!(
                lead.map_or(Value::Null, |topic| Value::String(topic.clone())),
                case["lead"],
                "fixture case: {name}"
            );
            let actual: Vec<Value> = sections
                .iter()
                .map(|section| serde_json::json!([section.group, section.index, section.total]))
                .collect();
            assert_eq!(Value::Array(actual), case["sections"], "fixture case: {name}");
        }
    }

    /// EXP-1154 — the fixture's `guide.captions`.
    #[test]
    fn guide_section_caption_matches_the_shared_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let cases = fixture["guide"]["captions"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(
                guide_section_caption(
                    case["index"].as_u64().unwrap() as usize,
                    case["total"].as_u64().unwrap() as usize,
                ),
                case["text"].as_str().unwrap()
            );
        }
    }

    /// EXP-1154 — the fixture's `guide.fileRows`: counts only for an exact
    /// path match in the loaded diff.
    #[test]
    fn guide_file_rows_matches_the_shared_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let cases = fixture["guide"]["fileRows"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let paths: Vec<String> = case["paths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| path.as_str().unwrap().to_string())
                .collect();
            let diff: Option<Vec<(String, u32, u32)>> = case["diff"].as_array().map(|files| {
                files
                    .iter()
                    .map(|file| {
                        (
                            file["path"].as_str().unwrap().to_string(),
                            file["additions"].as_u64().unwrap() as u32,
                            file["deletions"].as_u64().unwrap() as u32,
                        )
                    })
                    .collect()
            });
            let actual: Vec<Value> = guide_file_rows(&paths, diff.as_deref())
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "path": row.path,
                        "additions": row.counts.map(|(additions, _)| additions),
                        "deletions": row.counts.map(|(_, deletions)| deletions),
                    })
                })
                .collect();
            assert_eq!(Value::Array(actual), case["expected"], "fixture case: {name}");
        }
    }

    /// EXP-933 — the 60 cap counts PICTURES only: a topic's text filed after
    /// the cap still reaches its group.
    #[test]
    fn parse_session_result_groups_caps_pictures_but_keeps_later_text() {
        let mut items: Vec<Value> = (0..70)
            .map(|index| serde_json::json!({"topic": "Shots", "label": "Web", "attachmentId": format!("a{index}")}))
            .collect();
        items.push(serde_json::json!({"topic": "Summary", "text": "Done"}));
        let groups = parse_session_result_groups(Some(&Value::Array(items)));
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].entries.len(), MAX_SESSION_RESULTS);
        assert_eq!(groups[1].text.as_deref(), Some("Done"));
    }

    /// The ORDER of the blob is the order of the page, and one malformed
    /// entry costs that entry — never the whole face.
    #[test]
    fn parse_session_results_parses_a_flat_ordered_list_and_drops_malformed_entries() {
        let raw = serde_json::json!([
            {"topic": "Results page", "label": "Web", "attachmentId": "a1", "width": 1440, "height": 900},
            {"topic": "Results page", "label": "  ", "attachmentId": "a2"},
            {"topic": "", "label": "iOS", "attachmentId": "a3"},
            {"topic": "Results page", "label": "iOS", "attachmentId": ""},
            {"topic": "Results page", "label": "iOS"},
            "not an object",
            {"topic": "Results page", "label": "iOS", "attachmentId": "a4", "width": 0, "height": -3},
            {"topic": "Empty state", "label": "Android", "attachmentId": "a5", "width": 12.5, "height": 7},
        ]);
        let parsed = parse_session_results(Some(&raw));
        assert_eq!(
            parsed.iter().map(|e| e.attachment_id.as_str()).collect::<Vec<_>>(),
            vec!["a1", "a4", "a5"],
        );
        assert_eq!(parsed[0].topic, "Results page");
        assert_eq!(parsed[0].label, "Web");
        assert_eq!((parsed[0].width, parsed[0].height), (Some(1440), Some(900)));
        // Non-positive sides read as unknown, not as a bogus aspect.
        assert_eq!((parsed[1].width, parsed[1].height), (None, None));
        // A fractional side is not a pixel count.
        assert_eq!((parsed[2].width, parsed[2].height), (None, Some(7)));
    }

    /// A jsonb column hydrates out of SQLite as a STRING holding the array,
    /// unknown fields are simply ignored, and nothing usable reads as empty.
    #[test]
    fn parse_session_results_ignores_unknown_fields_and_a_null_or_blank_blob() {
        let as_string = serde_json::json!(
            r#"[{"topic":"Work header","label":"Desktop","attachmentId":"a1","kind":"screenshot","storageKey":"x"}]"#
        );
        let parsed = parse_session_results(Some(&as_string));
        assert_eq!(parsed, vec![entry("Work header", "Desktop", "a1")]);
        assert!(parse_session_results(None).is_empty());
        assert!(parse_session_results(Some(&Value::Null)).is_empty());
        assert!(parse_session_results(Some(&serde_json::json!("[]"))).is_empty());
        assert!(parse_session_results(Some(&serde_json::json!("not json"))).is_empty());
        assert!(parse_session_results(Some(&serde_json::json!({"topic": "x"}))).is_empty());
    }

    /// Topics band in the order they were first published — a later entry
    /// rejoins its topic rather than opening a second band for it.
    #[test]
    fn group_session_results_groups_by_topic_in_first_seen_order() {
        let entries = vec![
            entry("Run face", "Web", "a1"),
            entry("Results face", "Web", "a2"),
            entry("Run face", "iOS", "a3"),
            entry("Results face", "Android", "a4"),
        ];
        let groups = group_session_results(&entries);
        assert_eq!(
            groups.iter().map(|g| g.topic.as_str()).collect::<Vec<_>>(),
            vec!["Run face", "Results face"],
        );
        assert_eq!(
            groups[0].entries.iter().map(|e| e.attachment_id.as_str()).collect::<Vec<_>>(),
            vec!["a1", "a3"],
        );
        assert_eq!(
            groups[1].entries.iter().map(|e| e.attachment_id.as_str()).collect::<Vec<_>>(),
            vec!["a2", "a4"],
        );
        assert!(group_session_results(&[]).is_empty());
    }

    /// The server's cap, mirrored: an over-long blob renders its first 60,
    /// never nothing and never unbounded.
    #[test]
    fn parse_session_results_caps_at_60_entries() {
        let items: Vec<Value> = (0..80)
            .map(|index| {
                serde_json::json!({
                    "topic": "Shots",
                    "label": "Web",
                    "attachmentId": format!("a{index}"),
                })
            })
            .collect();
        let parsed = parse_session_results(Some(&Value::Array(items)));
        assert_eq!(parsed.len(), MAX_SESSION_RESULTS);
        assert_eq!(parsed[0].attachment_id, "a0");
        assert_eq!(parsed[MAX_SESSION_RESULTS - 1].attachment_id, "a59");
    }

    /// Equal-height tiles: the width follows the probed aspect, and a tile
    /// missing either side falls back to 4:3 rather than to a square.
    #[test]
    fn session_result_tile_width_sizes_a_tile_from_the_probed_aspect_4_3_without_one() {
        let mut shot = entry("Shots", "Web", "a1");
        shot.width = Some(1440);
        shot.height = Some(900);
        assert_eq!(
            session_result_tile_width(&shot, SESSION_RESULT_TILE_HEIGHT),
            512.0,
        );
        // A phone shot is TALLER than it is wide — the tile narrows.
        let mut phone = entry("Shots", "iOS", "a2");
        phone.width = Some(1170);
        phone.height = Some(2532);
        assert_eq!(session_result_tile_width(&phone, 320.0), 148.0);
        // Either side unknown = 4:3.
        let unknown = entry("Shots", "Android", "a3");
        assert_eq!(session_result_tile_width(&unknown, 300.0), 400.0);
        let mut half = entry("Shots", "Android", "a4");
        half.width = Some(800);
        assert_eq!(session_result_tile_width(&half, 300.0), 400.0);
    }

    /// EXP-1128: the tall rule, the fixture's `tiles` cases ×4 — a full-page
    /// capture flags tall and takes the 4:3 frame; a phone shot never does.
    #[test]
    fn session_result_tile_width_frames_a_tall_capture_at_4_3_and_flags_it() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let cases = fixture["tiles"]["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut shot = entry("Shots", "Web", "a1");
            shot.width = case["width"].as_u64().map(|value| value as u32);
            shot.height = case["height"].as_u64().map(|value| value as u32);
            assert_eq!(
                session_result_is_tall(&shot),
                case["tall"].as_bool().unwrap(),
                "fixture case: {name}"
            );
            assert_eq!(
                session_result_tile_width(&shot, SESSION_RESULT_TILE_HEIGHT),
                case["widthAt320"].as_f64().unwrap() as f32,
                "fixture case: {name}"
            );
        }
    }

    /// The page keeps the base height until the WIDEST tile would overflow
    /// the row; then every tile shrinks by the SAME factor, so the shared
    /// baseline survives a narrow window (and a phone on the other clients).
    #[test]
    fn session_result_tile_height_fitting_scales_every_tile_down_by_one_factor_when_the_widest_overflows_the_page(
    ) {
        let mut landscape = entry("Shots", "Web", "a1");
        landscape.width = Some(1800);
        landscape.height = Some(1200);
        let mut phone = entry("Shots", "iOS", "a2");
        phone.width = Some(828);
        phone.height = Some(1800);
        let entries = vec![landscape, phone];
        // The landscape tile is 480 wide at the base height — too wide for a
        // 358 px row, so the page scales down: 320 * 358 / 480, floored.
        assert_eq!(session_result_tile_height_fitting(&entries, 358.0), 238.0);
        // Room for the widest tile = the base height, untouched.
        assert_eq!(
            session_result_tile_height_fitting(&entries, 1000.0),
            SESSION_RESULT_TILE_HEIGHT,
        );
        // An unmeasured width is not a constraint.
        assert_eq!(
            session_result_tile_height_fitting(&entries, 0.0),
            SESSION_RESULT_TILE_HEIGHT,
        );
        assert_eq!(
            session_result_tile_height_fitting(&[], 358.0),
            SESSION_RESULT_TILE_HEIGHT,
        );
    }
}
