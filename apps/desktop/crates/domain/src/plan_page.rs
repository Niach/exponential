//! #EXP-1493 THE PLAN PAGE ×4 (`packages/domain-contract/src/plan-page.ts`'s
//! twin, iOS ExpCore `PlanPage.swift`, Android `domain/PlanPage.kt`): a
//! plan's markdown split into blocks (prose, h2 sections, picture groups),
//! its outline, its counts line, the wide picture group's tiles and where a
//! session's waiting plan opens. #EXP-1662: the plan is a TAB of its chat:
//! the `title` block, the row's and the chip's label, the tab's target.
//! Byte-locked by `fixtures/plan-page.json` (its `$comment` is the spec); pure.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PLAN_PAGE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/plan-page.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanPageCounts {
    pub sections: [String; 2],
    pub pictures: [String; 2],
    pub questions: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPageCopy {
    pub title: String,
    pub open_plan: String,
    pub open_full: String,
    pub open_chat: String,
    pub outline: String,
    pub questions: String,
    pub counts: PlanPageCounts,
    pub needs_you: String,
    pub row_prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PlanPagePadding {
    pub wide: f32,
    pub phone: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPageLayout {
    pub outline_width: f32,
    pub column_max_width: f32,
    pub column_padding: PlanPagePadding,
    pub block_gap: f32,
    pub outline_row_height: f32,
    pub outline_active_bar: f32,
    pub title_size: f32,
    pub row_height: f32,
    pub tab_max_width: f32,
    /// #FEED-191: the wide questions rail's width.
    pub rail_width: f32,
    /// #EXP-1786: the document's readable floor; the outline, then the rail
    /// yield below it (`plan_pane_layout`).
    pub document_min_width: f32,
}

/// #EXP-1662: the floating questions panel (`panel`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPagePanel {
    pub max_width: f32,
    pub gap: f32,
    pub padding: f32,
    pub radius: f32,
    pub max_height_fraction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MosaicSpec {
    pub gap: f32,
    pub large_fraction: f32,
    pub aspect: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPageTestIds {
    pub page: String,
    pub outline: String,
    pub outline_row: String,
    pub document: String,
    pub rail: String,
    pub mosaic: String,
    pub carousel: String,
    pub open_plan: String,
    pub outline_toggle: String,
    pub tab: String,
}

/// THE styleguide specimen ×4 (`specimen`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPageSpecimen {
    pub chat_name: String,
    pub active_section: String,
    pub card: Value,
}

/// `fixtures/plan-page.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPageSpec {
    pub route: String,
    pub copy: PlanPageCopy,
    pub layout: PlanPageLayout,
    pub mosaic: MosaicSpec,
    pub test_ids: PlanPageTestIds,
    pub specimen: PlanPageSpecimen,
    pub panel: PlanPagePanel,
}

pub static PLAN_PAGE: LazyLock<PlanPageSpec> =
    LazyLock::new(|| serde_json::from_str(PLAN_PAGE_JSON).expect("plan-page.json parses"));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanPicture {
    pub alt: String,
    pub src: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanBlock {
    /// #EXP-1662: a leading `# ` line (plain text), the document's big title.
    Title { text: String },
    Markdown { text: String },
    Section { id: String, title: String },
    Pictures { pictures: Vec<PlanPicture> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSection {
    pub id: String,
    pub title: String,
    /// Pictures inside the section (grouped or single).
    pub pictures: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCounts {
    pub sections: usize,
    pub pictures: usize,
    pub questions: usize,
}

static PICTURE_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*!\[([^\]]*)\]\(([^)\s]+)\)\s*$").unwrap());
static PICTURE_ANY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\(([^)\s]+)\)").unwrap());
static H1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\s+(.+?)\s*#*\s*$").unwrap());
static PLAN_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^plan:\s*").unwrap());
static H2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^##\s+(.+?)\s*#*\s*$").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(```|~~~)").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static NON_SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9]+").unwrap());
const PENDING: &str = "upload:";

/// A heading's anchor: lowercase, runs of anything but a-z0-9 = `-`.
pub fn section_slug(title: &str) -> String {
    let slug = NON_SLUG.replace_all(&title.to_lowercase(), "-").trim_matches('-').to_string();
    if slug.is_empty() { "section".to_string() } else { slug }
}

fn plain_title(raw: &str) -> String {
    LINK.replace_all(raw, "${1}").replace("**", "").replace("__", "").replace('`', "").trim().to_string()
}

fn flush_prose(blocks: &mut Vec<PlanBlock>, prose: &mut Vec<String>) {
    let start = prose.iter().position(|line| !line.trim().is_empty());
    let end = prose.iter().rposition(|line| !line.trim().is_empty());
    if let (Some(start), Some(end)) = (start, end) {
        blocks.push(PlanBlock::Markdown { text: prose[start..=end].join("\n") });
    }
    prose.clear();
}

/// #EXP-1662 / #FEED-191: the plan a chat's tab shows (`tabCases`; `plans`
/// = (id, decision), oldest first): the newest waiting one, else `kept` while
/// it is an approved or rejected plan of the chat (the plan the tab showed
/// during THIS visit of the chat; the host clears it when the person leaves),
/// else none: a decided plan leaves the strip.
pub fn plan_tab_target<'a>(plans: &[(&'a str, Option<&str>)], kept: Option<&str>) -> Option<&'a str> {
    if let Some((id, _)) = plans.iter().rev().find(|(_, decision)| decision.is_none()) {
        return Some(*id);
    }
    let kept = kept?;
    plans
        .iter()
        .find(|(id, _)| *id == kept)
        .filter(|(_, decision)| matches!(decision, Some("approved" | "rejected")))
        .map(|(id, _)| *id)
}

/// #FEED-191: the plan tab's lead glyph (`tabGlyphCases`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlanTabGlyph {
    /// `coding-plan` | `ui-check`.
    pub icon: &'static str,
    /// `text` = the chip's own text colour, `approved` = emerald (the plan
    /// row's approved check).
    pub tone: &'static str,
}

/// #FEED-191: an approved plan's chip leads with THE check in emerald (as its
/// row), anything else with the plan glyph.
pub fn plan_tab_glyph(decision: Option<&str>) -> PlanTabGlyph {
    if decision == Some("approved") {
        PlanTabGlyph { icon: "ui-check", tone: "approved" }
    } else {
        PlanTabGlyph { icon: "coding-plan", tone: "text" }
    }
}

/// #EXP-1786: which wide columns the plan body's OWN width holds
/// (`paneCases`). `outline` false = no outline column; `rail` false = the
/// questions float (`panel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlanPaneLayout {
    pub outline: bool,
    pub rail: bool,
}

/// #EXP-1786: the three columns while the document keeps
/// `layout.documentMinWidth` beside both, else the document + the rail, else
/// the document alone with the floating questions.
pub fn plan_pane_layout(width: f32) -> PlanPaneLayout {
    let layout = &PLAN_PAGE.layout;
    PlanPaneLayout {
        outline: width >= layout.outline_width + layout.rail_width + layout.document_min_width,
        rail: width >= layout.rail_width + layout.document_min_width,
    }
}

/// #EXP-1662: `Plan: title`, the row's and the wide tab's label
/// (`rowLabelCases`).
pub fn plan_row_label(plan: &str) -> String {
    let title = crate::ask_dock::ask_dock_title(plan);
    let title = PLAN_PREFIX.replace(&title, "");
    if title.is_empty() {
        PLAN_PAGE.copy.title.clone()
    } else {
        format!("{}{title}", PLAN_PAGE.copy.row_prefix)
    }
}

/// THE plan page's blocks (`blockCases`): a leading `# ` line = the `title`
/// block (every other `# ` stays in the prose), then the markdown in order, every `## `
/// line outside a code fence = a `section` block (its anchor deduped `-2`,
/// `-3`…), a run of two or more picture-only lines (blank lines between
/// them allowed) = ONE `pictures` block; a lone picture stays in the prose.
/// A pending `upload:` picture renders nothing: it is dropped. Prose blocks
/// are trimmed of blank edge lines; an empty one is dropped.
pub fn plan_blocks(markdown: &str) -> Vec<PlanBlock> {
    let mut blocks = Vec::new();
    let mut lines: Vec<&str> = markdown.split('\n').collect();
    if let Some(first) = lines.iter().position(|line| !line.trim().is_empty()) {
        if let Some(heading) = H1.captures(lines[first]) {
            blocks.push(PlanBlock::Title { text: plain_title(&heading[1]) });
            lines.remove(first);
        }
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut prose: Vec<String> = Vec::new();
    let mut run: Vec<PlanPicture> = Vec::new();
    let mut run_lines: Vec<String> = Vec::new();
    let mut fenced = false;

    let flush_run =
        |blocks: &mut Vec<PlanBlock>, prose: &mut Vec<String>, run: &mut Vec<PlanPicture>, run_lines: &mut Vec<String>| {
            if run.len() >= 2 {
                flush_prose(blocks, prose);
                blocks.push(PlanBlock::Pictures { pictures: std::mem::take(run) });
            } else {
                prose.append(run_lines);
            }
            run.clear();
            run_lines.clear();
        };

    for line in lines {
        if FENCE.is_match(line) {
            flush_run(&mut blocks, &mut prose, &mut run, &mut run_lines);
            fenced = !fenced;
            prose.push(line.to_string());
            continue;
        }
        if fenced {
            prose.push(line.to_string());
            continue;
        }
        if let Some(picture) = PICTURE_LINE.captures(line) {
            let src = &picture[2];
            if src.starts_with(PENDING) {
                continue;
            }
            run.push(PlanPicture { alt: picture[1].to_string(), src: src.to_string() });
            run_lines.push(line.to_string());
            continue;
        }
        if line.trim().is_empty() && !run.is_empty() {
            run_lines.push(line.to_string());
            continue;
        }
        flush_run(&mut blocks, &mut prose, &mut run, &mut run_lines);
        if let Some(heading) = H2.captures(line) {
            flush_prose(&mut blocks, &mut prose);
            let title = plain_title(&heading[1]);
            let base = section_slug(&title);
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            let id = if *n == 1 { base } else { format!("{base}-{n}") };
            blocks.push(PlanBlock::Section { id, title });
            continue;
        }
        let kept = PICTURE_ANY.replace_all(line, |caps: &regex::Captures| {
            if caps[1].starts_with(PENDING) { String::new() } else { caps[0].to_string() }
        });
        prose.push(kept.into_owned());
    }
    flush_run(&mut blocks, &mut prose, &mut run, &mut run_lines);
    flush_prose(&mut blocks, &mut prose);
    blocks
}

/// The pictures a block shows (a fenced code block shows none).
fn pictures_in(block: &PlanBlock) -> usize {
    match block {
        PlanBlock::Pictures { pictures } => pictures.len(),
        PlanBlock::Title { .. } | PlanBlock::Section { .. } => 0,
        PlanBlock::Markdown { text } => {
            let mut fenced = false;
            let mut count = 0;
            for line in text.split('\n') {
                if FENCE.is_match(line) {
                    fenced = !fenced;
                } else if !fenced {
                    count += PICTURE_ANY.captures_iter(line).filter(|caps| !caps[1].starts_with(PENDING)).count();
                }
            }
            count
        }
    }
}

/// THE outline (`outlineCases`): one row per `section` block, in order, with
/// the pictures between it and the next section.
pub fn plan_outline(markdown: &str) -> Vec<PlanSection> {
    let mut out: Vec<PlanSection> = Vec::new();
    for block in plan_blocks(markdown) {
        match &block {
            PlanBlock::Section { id, title } => out.push(PlanSection { id: id.clone(), title: title.clone(), pictures: 0 }),
            _ => {
                if let Some(last) = out.last_mut() {
                    last.pictures += pictures_in(&block);
                }
            }
        }
    }
    out
}

/// A plan card's counts (`countCases`): its h2 sections, its RESOLVED
/// pictures (attachmentId set), its questions.
pub fn plan_counts(card: &Value) -> PlanCounts {
    let pictures = card
        .get("pictures")
        .and_then(Value::as_array)
        .map_or(0, |list| list.iter().filter(|p| p.get("attachmentId").is_some_and(Value::is_string)).count());
    PlanCounts {
        sections: plan_outline(card.get("plan").and_then(Value::as_str).unwrap_or_default()).len(),
        pictures,
        questions: card.get("questions").and_then(Value::as_array).map_or(0, Vec::len),
    }
}

/// "5 sections · 10 pictures · 7 questions": zero parts left out, the
/// singular for one; nothing = ``.
pub fn plan_counts_line(counts: &PlanCounts) -> String {
    let copy = &PLAN_PAGE.copy.counts;
    [(counts.sections, &copy.sections), (counts.pictures, &copy.pictures), (counts.questions, &copy.questions)]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, words)| format!("{n} {}", words[if n == 1 { 0 } else { 1 }]))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MosaicTileSize {
    /// The left tile, two rows tall.
    Large,
    /// One of a pair.
    Half,
    /// A right tile.
    Small,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MosaicTile {
    /// The picture's index in the group.
    pub index: usize,
    pub size: MosaicTileSize,
    /// "+N" on the last small tile when the group has more than it shows.
    pub more: usize,
}

/// THE wide picture group (eUI `Mosaic`, `mosaicCases`): two = two halves;
/// three or more = the first large on the left, the next two small stacked
/// on the right, the last of them carrying "+N".
pub fn mosaic_tiles(count: usize) -> Vec<MosaicTile> {
    let tile = |index, size, more| MosaicTile { index, size, more };
    match count {
        0 => Vec::new(),
        1 => vec![tile(0, MosaicTileSize::Large, 0)],
        2 => vec![tile(0, MosaicTileSize::Half, 0), tile(1, MosaicTileSize::Half, 0)],
        _ => vec![
            tile(0, MosaicTileSize::Large, 0),
            tile(1, MosaicTileSize::Small, 0),
            tile(2, MosaicTileSize::Small, count - 3),
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPageMessage {
    pub id: String,
    pub session_id: String,
    pub author_session_id: Option<String>,
    /// Epoch ms.
    pub created_at: i64,
    pub card: Value,
}

/// Is this card a plan the page opens (`planPageFor`'s card rule)?
pub fn is_waiting_own_plan(card: &Value) -> bool {
    card.get("kind").and_then(Value::as_str) == Some("plan")
        && card.get("decision").is_none_or(Value::is_null)
        && !card.get("questionId").is_some_and(Value::is_string)
}

/// Where a session's waiting plan opens (`targetCases`): the NEWEST plan
/// card the session posted itself (author = session, no engine
/// `questionId`) with no decision; `None` = none (the host opens the
/// session).
pub fn plan_page_for<'a>(session_id: &str, messages: &'a [PlanPageMessage]) -> Option<&'a str> {
    messages
        .iter()
        .filter(|m| m.session_id == session_id && m.author_session_id.as_deref() == Some(session_id))
        .filter(|m| is_waiting_own_plan(&m.card))
        .max_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)))
        .map(|m| m.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(PLAN_PAGE_JSON).unwrap()
    }

    #[test]
    fn spec_parses() {
        assert_eq!(PLAN_PAGE.copy.open_plan, "Open plan");
        assert_eq!(PLAN_PAGE.layout.outline_width, 240.0);
        assert_eq!(PLAN_PAGE.mosaic.large_fraction, 0.6);
        assert_eq!(PLAN_PAGE.test_ids.page, "plan-page");
        assert_eq!(PLAN_PAGE.test_ids.tab, "chat-tab-plan");
        assert_eq!(PLAN_PAGE.copy.needs_you, "Needs you");
        assert_eq!(PLAN_PAGE.layout.title_size, 28.0);
        assert_eq!(PLAN_PAGE.layout.row_height, 44.0);
        assert_eq!(PLAN_PAGE.layout.tab_max_width, 260.0);
        assert_eq!(PLAN_PAGE.layout.rail_width, 360.0);
        assert_eq!(PLAN_PAGE.layout.document_min_width, 360.0);
        assert_eq!(PLAN_PAGE.panel.max_width, 720.0);
        assert_eq!(PLAN_PAGE.panel.max_height_fraction, 0.4);
    }

    #[test]
    fn blocks_match_the_fixture() {
        for case in fixture()["blockCases"].as_array().unwrap() {
            let actual = serde_json::to_value(plan_blocks(case["markdown"].as_str().unwrap())).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn outline_matches_the_fixture() {
        for case in fixture()["outlineCases"].as_array().unwrap() {
            let actual = serde_json::to_value(plan_outline(case["markdown"].as_str().unwrap())).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn counts_match_the_fixture() {
        let fixture = fixture();
        for case in fixture["countCases"].as_array().unwrap() {
            let actual = serde_json::to_value(plan_counts(&case["card"])).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
        for case in fixture["countsLineCases"].as_array().unwrap() {
            let counts: PlanCounts = serde_json::from_value(case["counts"].clone()).unwrap();
            assert_eq!(plan_counts_line(&counts), case["expected"].as_str().unwrap());
        }
    }

    #[test]
    fn mosaic_matches_the_fixture() {
        for case in fixture()["mosaicCases"].as_array().unwrap() {
            let actual = serde_json::to_value(mosaic_tiles(case["count"].as_u64().unwrap() as usize)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["count"]);
        }
    }

    #[test]
    fn tab_target_matches_the_fixture() {
        for case in fixture()["tabCases"].as_array().unwrap() {
            let plans: Vec<(&str, Option<&str>)> = case["plans"]
                .as_array()
                .unwrap()
                .iter()
                .map(|plan| (plan["id"].as_str().unwrap(), plan["decision"].as_str()))
                .collect();
            assert_eq!(plan_tab_target(&plans, case["kept"].as_str()), case["expect"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn tab_glyph_matches_the_fixture() {
        let cases = fixture()["tabGlyphCases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let actual = serde_json::to_value(plan_tab_glyph(case["decision"].as_str())).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["decision"]);
        }
    }

    #[test]
    fn pane_layout_matches_the_fixture() {
        let cases = fixture()["paneCases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let actual = serde_json::to_value(plan_pane_layout(case["width"].as_f64().unwrap() as f32)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn row_label_matches_the_fixture() {
        for case in fixture()["rowLabelCases"].as_array().unwrap() {
            assert_eq!(plan_row_label(case["plan"].as_str().unwrap()), case["expect"].as_str().unwrap());
        }
    }

    #[test]
    fn target_matches_the_fixture() {
        for case in fixture()["targetCases"].as_array().unwrap() {
            let messages: Vec<PlanPageMessage> = serde_json::from_value(case["messages"].clone()).unwrap();
            let actual = plan_page_for(case["sessionId"].as_str().unwrap(), &messages);
            assert_eq!(actual, case["expected"].as_str(), "{}", case["name"]);
        }
    }
}
