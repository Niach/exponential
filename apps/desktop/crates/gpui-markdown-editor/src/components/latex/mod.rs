//! LaTeX display-math parsing helpers.
//!
//! EXP-261 vendoring: the RaTeX SVG rendering pipeline (and its `ratex-*` +
//! `directories` dependencies, including the on-disk SVG cache) was removed.
//! The document parser no longer recognizes math blocks or inline math, so the
//! render entry points below are unreachable; they remain only so the block
//! render paths keep compiling and fail soft if ever reached.

use std::path::PathBuf;

use anyhow::bail;
use gpui::Hsla;

const DISPLAY_MATH_SCALE: f32 = 1.25;
const INLINE_MATH_SCALE: f32 = 1.12;

/// Parsed display-math source preserved from Markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayMathSource {
    /// Full Markdown source, including `$$` delimiters.
    pub(crate) raw: String,
    /// LaTeX body between the display delimiters.
    pub(crate) body: String,
}

/// Result of rendering display math into an SVG cache file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LatexSvgRender {
    /// Path to the SVG file consumed by GPUI's image element.
    pub(crate) path: PathBuf,
    /// SVG document content, used by export paths.
    pub(crate) svg: String,
}

/// Parse a raw `$$...$$` Markdown block into the LaTeX body it contains.
pub(crate) fn parse_display_math_source(raw: &str) -> Option<DisplayMathSource> {
    let raw = raw.trim_matches('\n').to_string();
    let lines = raw.split('\n').collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }

    if lines.len() == 1 {
        let line = strip_display_indent(lines[0])?.trim_end();
        let body_and_close = line.strip_prefix("$$")?;
        let close = body_and_close.find("$$")?;
        let body = body_and_close[..close].trim().to_string();
        return Some(DisplayMathSource { raw, body });
    }

    let opener = strip_display_indent(lines[0])?.trim_end();
    let closer = lines.last()?.trim();
    if opener != "$$" || closer != "$$" {
        return None;
    }

    let body = lines[1..lines.len() - 1].join("\n");
    Some(DisplayMathSource { raw, body })
}

/// Display font size used for rendered display-math blocks.
pub(crate) fn display_math_font_size(base_font_size: f32) -> f32 {
    base_font_size * DISPLAY_MATH_SCALE
}

/// Display font size used for rendered inline math.
pub(crate) fn inline_math_font_size(base_font_size: f32) -> f32 {
    base_font_size * INLINE_MATH_SCALE
}

/// Render a display-math source into a cached SVG file.
///
/// Unreachable after the EXP-261 excision — math blocks are never parsed.
pub(crate) fn render_display_math_svg(
    _source: &DisplayMathSource,
    _text_color: Hsla,
    _font_size: f32,
) -> anyhow::Result<LatexSvgRender> {
    bail!("LaTeX rendering was removed in this vendored build")
}

/// Render an inline LaTeX body into a cached SVG file.
///
/// Unreachable after the EXP-261 excision — inline math is never parsed.
pub(crate) fn render_inline_math_svg(
    _latex: &str,
    _text_color: Hsla,
    _font_size: f32,
) -> anyhow::Result<LatexSvgRender> {
    bail!("LaTeX rendering was removed in this vendored build")
}

fn strip_display_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_line_display_math() {
        let parsed = parse_display_math_source("$$x^2$$").expect("display math");
        assert_eq!(parsed.body, "x^2");
        assert_eq!(parsed.raw, "$$x^2$$");
    }

    #[test]
    fn parses_multiline_display_math() {
        let parsed = parse_display_math_source("$$\n\\int_0^1 x^2 dx\n$$").expect("display math");
        assert_eq!(parsed.body, "\\int_0^1 x^2 dx");
    }

    #[test]
    fn rejects_unclosed_display_math() {
        assert!(parse_display_math_source("$$\n\\frac{1}{2}").is_none());
    }

    #[test]
    fn display_math_font_size_scales_base_text_size() {
        assert_eq!(display_math_font_size(20.0), 25.0);
    }

    #[test]
    fn inline_math_font_size_scales_base_text_size() {
        assert!((inline_math_font_size(20.0) - 22.4).abs() < 0.001);
    }
}
