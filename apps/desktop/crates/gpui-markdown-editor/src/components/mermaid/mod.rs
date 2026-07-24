//! Mermaid fenced-block parsing helpers.
//!
//! EXP-261 vendoring: the Mermaid SVG rendering pipeline (and its
//! `mermaid-rs-renderer` + `directories` dependencies, including the on-disk
//! SVG cache) was removed. The document parser no longer classifies
//! ```` ```mermaid ```` fences as Mermaid blocks — they parse as ordinary
//! fenced code blocks — so the render entry point below is unreachable; it
//! remains only so the block render path keeps compiling and fails soft if
//! ever reached.

use std::path::PathBuf;

use anyhow::bail;

/// Opening fence metadata for a Mermaid fenced code block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MermaidFence {
    /// Fence marker, either backtick or tilde.
    pub(crate) marker: char,
    /// Opening fence run length.
    pub(crate) len: usize,
}

/// Parsed Mermaid source preserved from Markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MermaidSource {
    /// Full Markdown source, including the opening and closing fences.
    pub(crate) raw: String,
    /// Mermaid diagram source between the fences.
    pub(crate) body: String,
    /// The full info string after the opening fence.
    pub(crate) info: String,
}

/// Result of rendering a Mermaid diagram into an SVG cache file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MermaidSvgRender {
    /// Path to the SVG file consumed by GPUI's image element.
    pub(crate) path: PathBuf,
    /// SVG document content, used by export paths.
    pub(crate) svg: String,
    /// Concrete display width encoded into the cached SVG.
    pub(crate) display_width: f32,
    /// Concrete display height encoded into the cached SVG.
    pub(crate) display_height: f32,
    /// Scale applied to the renderer's intrinsic SVG size for editor display.
    pub(crate) display_scale: f32,
}

/// Returns true when a fenced code info string declares Mermaid content.
pub(crate) fn is_mermaid_info_string(info: Option<&str>) -> bool {
    info.and_then(|info| info.split_whitespace().next())
        .is_some_and(|first| {
            first.eq_ignore_ascii_case("mermaid") || first.eq_ignore_ascii_case("mmd")
        })
}

/// Parse a line as a Mermaid opening fence.
pub(crate) fn parse_mermaid_fence_start(line: &str) -> Option<MermaidFence> {
    let trimmed = strip_fence_indent(line)?.trim_end();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let len = trimmed.chars().take_while(|ch| *ch == marker).count();
    if len < 3 {
        return None;
    }

    let info = trimmed[marker.len_utf8() * len..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }

    is_mermaid_info_string((!info.is_empty()).then_some(info))
        .then_some(MermaidFence { marker, len })
}

/// Returns true when `line` closes the given Mermaid fence.
pub(crate) fn is_mermaid_closing_fence(line: &str, fence: MermaidFence) -> bool {
    let Some(trimmed) = strip_fence_indent(line).map(str::trim_end) else {
        return false;
    };
    if !trimmed.starts_with(fence.marker) {
        return false;
    }

    let len = trimmed.chars().take_while(|ch| *ch == fence.marker).count();
    len >= fence.len && trimmed[fence.marker.len_utf8() * len..].trim().is_empty()
}

/// Parse raw fenced Markdown into the Mermaid diagram source it contains.
pub(crate) fn parse_mermaid_fence_source(raw: &str) -> Option<MermaidSource> {
    let raw = raw.trim_matches('\n').to_string();
    let lines = raw.split('\n').collect::<Vec<_>>();
    if lines.len() < 2 {
        return None;
    }

    let opening = strip_fence_indent(lines[0])?.trim_end();
    let fence = parse_mermaid_fence_start(opening)?;
    let info = opening[fence.marker.len_utf8() * fence.len..]
        .trim()
        .to_string();
    if !is_mermaid_closing_fence(lines.last()?, fence) {
        return None;
    }

    let body = lines[1..lines.len() - 1].join("\n");
    Some(MermaidSource { raw, body, info })
}

/// Render Mermaid source into a cached SVG sized for editor display.
///
/// Unreachable after the EXP-261 excision — Mermaid blocks are never parsed.
pub(crate) fn render_mermaid_svg_for_display(
    _source: &MermaidSource,
    _available_width: f32,
    _viewport_width: f32,
) -> anyhow::Result<MermaidSvgRender> {
    bail!("Mermaid rendering was removed in this vendored build")
}

fn strip_fence_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_mermaid_info_strings() {
        assert!(is_mermaid_info_string(Some("mermaid")));
        assert!(is_mermaid_info_string(Some("MMD")));
        assert!(!is_mermaid_info_string(Some("rust")));
        assert!(!is_mermaid_info_string(None));
    }

    #[test]
    fn parses_mermaid_fence_source() {
        let source =
            parse_mermaid_fence_source("```mermaid\ngraph TD;\nA-->B;\n```").expect("mermaid");
        assert_eq!(source.body, "graph TD;\nA-->B;");
        assert_eq!(source.info, "mermaid");
    }

    #[test]
    fn rejects_unclosed_mermaid_fence() {
        assert!(parse_mermaid_fence_source("```mermaid\ngraph TD;").is_none());
    }
}
