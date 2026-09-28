//! Hand parser for the grid strings the whitelist admits:
//! `gridTemplateColumns/Rows` = `<n>px | <n> | <n>% | <n>fr | auto | min-content |
//! max-content | minmax(a, b) | repeat(n, …)`, `gridTemplateAreas` = string rows,
//! `gridArea` = area name, `gridColumn/gridRow` = `a`, `a / b`, `span n`, name.

use taffy::prelude::*;
use taffy::style::{
    GridPlacement, GridTemplateArea, GridTemplateComponent, GridTemplateRepetition,
    MaxTrackSizingFunction, MinTrackSizingFunction, RepetitionCount, TrackSizingFunction,
};

/// Split at top-level whitespace and commas (commas only inside `minmax`/`repeat`
/// argument lists, which the callers split separately).
fn split_top_level(s: &str, sep_comma: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if sep_comma && depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            c if c.is_whitespace() && depth == 0 && !sep_comma => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn number_prefix(s: &str, suffix: &str) -> Option<f32> {
    s.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok())
}

fn min_fn(tok: &str) -> Result<MinTrackSizingFunction, String> {
    Ok(match tok {
        "auto" => MinTrackSizingFunction::auto(),
        "min-content" => MinTrackSizingFunction::min_content(),
        "max-content" => MinTrackSizingFunction::max_content(),
        t => {
            if let Some(v) = number_prefix(t, "px") {
                MinTrackSizingFunction::length(v)
            } else if let Some(v) = number_prefix(t, "%") {
                MinTrackSizingFunction::percent(v / 100.0)
            } else if let Ok(v) = t.parse::<f32>() {
                MinTrackSizingFunction::length(v)
            } else {
                return Err(format!("unsupported min track size {t:?}"));
            }
        }
    })
}

fn max_fn(tok: &str) -> Result<MaxTrackSizingFunction, String> {
    Ok(match tok {
        "auto" => MaxTrackSizingFunction::auto(),
        "min-content" => MaxTrackSizingFunction::min_content(),
        "max-content" => MaxTrackSizingFunction::max_content(),
        t => {
            if let Some(v) = number_prefix(t, "fr") {
                MaxTrackSizingFunction::fr(v)
            } else if let Some(v) = number_prefix(t, "px") {
                MaxTrackSizingFunction::length(v)
            } else if let Some(v) = number_prefix(t, "%") {
                MaxTrackSizingFunction::percent(v / 100.0)
            } else if let Ok(v) = t.parse::<f32>() {
                MaxTrackSizingFunction::length(v)
            } else {
                return Err(format!("unsupported max track size {t:?}"));
            }
        }
    })
}

/// One non-repeated track: `1fr` becomes `minmax(auto, 1fr)` like CSS.
fn track(tok: &str) -> Result<TrackSizingFunction, String> {
    if let Some(inner) = tok.strip_prefix("minmax(").and_then(|s| s.strip_suffix(')')) {
        let parts = split_top_level(inner, true);
        if parts.len() != 2 {
            return Err(format!("minmax needs two arguments: {tok:?}"));
        }
        return Ok(minmax(min_fn(&parts[0])?, max_fn(&parts[1])?));
    }
    if tok.ends_with("fr") {
        return Ok(minmax(MinTrackSizingFunction::auto(), max_fn(tok)?));
    }
    Ok(minmax(min_fn(tok)?, max_fn(tok)?))
}

/// `gridTemplateColumns` / `gridTemplateRows`.
pub fn parse_tracks(s: &str) -> Result<Vec<GridTemplateComponent<String>>, String> {
    let mut out = Vec::new();
    for tok in split_top_level(s, false) {
        if let Some(inner) = tok.strip_prefix("repeat(").and_then(|s| s.strip_suffix(')')) {
            let parts = split_top_level(inner, true);
            if parts.len() != 2 {
                return Err(format!("repeat needs two arguments: {tok:?}"));
            }
            let count = match parts[0].as_str() {
                "auto-fill" => RepetitionCount::AutoFill,
                "auto-fit" => RepetitionCount::AutoFit,
                n => RepetitionCount::Count(
                    n.parse::<u16>().map_err(|_| format!("bad repeat count {n:?}"))?,
                ),
            };
            let tracks = split_top_level(&parts[1], false)
                .iter()
                .map(|t| track(t))
                .collect::<Result<Vec<_>, _>>()?;
            let line_names = vec![Vec::new(); tracks.len() + 1];
            out.push(GridTemplateComponent::Repeat(GridTemplateRepetition {
                count,
                tracks,
                line_names,
            }));
        } else {
            out.push(GridTemplateComponent::Single(track(&tok)?));
        }
    }
    Ok(out)
}

/// `gridTemplateAreas` rows → named areas (1-based lines, end exclusive).
/// `.` is an empty cell. Non-rectangular areas are rejected.
pub fn parse_areas(rows: &[String]) -> Result<Vec<GridTemplateArea<String>>, String> {
    let mut areas: Vec<GridTemplateArea<String>> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, name) in row.split_whitespace().enumerate() {
            if name == "." {
                continue;
            }
            let row_line = (r + 1) as u16;
            let col_line = (c + 1) as u16;
            if let Some(a) = areas.iter_mut().find(|a| a.name == name) {
                a.row_start = a.row_start.min(row_line);
                a.row_end = a.row_end.max(row_line + 1);
                a.column_start = a.column_start.min(col_line);
                a.column_end = a.column_end.max(col_line + 1);
            } else {
                areas.push(GridTemplateArea {
                    name: name.to_string(),
                    row_start: row_line,
                    row_end: row_line + 1,
                    column_start: col_line,
                    column_end: col_line + 1,
                });
            }
        }
    }
    // Rectangularity: every cell inside the bounding box must carry the name.
    for a in &areas {
        for r in a.row_start..a.row_end {
            let cells: Vec<&str> = rows
                .get((r - 1) as usize)
                .map(|row| row.split_whitespace().collect())
                .unwrap_or_default();
            for c in a.column_start..a.column_end {
                if cells.get((c - 1) as usize).copied() != Some(a.name.as_str()) {
                    return Err(format!("grid area {:?} is not rectangular", a.name));
                }
            }
        }
    }
    Ok(areas)
}

fn placement(tok: &str) -> Result<GridPlacement<String>, String> {
    let tok = tok.trim();
    if tok == "auto" {
        return Ok(GridPlacement::Auto);
    }
    if let Some(n) = tok.strip_prefix("span ") {
        return Ok(GridPlacement::Span(
            n.trim().parse::<u16>().map_err(|_| format!("bad span {tok:?}"))?,
        ));
    }
    if let Ok(n) = tok.parse::<i16>() {
        return Ok(line(n));
    }
    Ok(GridPlacement::NamedLine(tok.to_string(), 0))
}

/// `gridColumn` / `gridRow`: `"2"`, `"1 / 3"`, `"span 2"`, `"nav"`.
pub fn parse_placement(s: &str) -> Result<Line<GridPlacement<String>>, String> {
    let parts: Vec<&str> = s.split('/').map(str::trim).collect();
    match parts.as_slice() {
        [one] => {
            let start = placement(one)?;
            let end = match &start {
                // A lone named line means the whole area, like `grid-area: nav`.
                GridPlacement::NamedLine(name, i) => GridPlacement::NamedLine(name.clone(), *i),
                GridPlacement::Span(_) => start.clone(),
                _ => GridPlacement::Auto,
            };
            Ok(Line { start, end })
        }
        [a, b] => Ok(Line { start: placement(a)?, end: placement(b)? }),
        _ => Err(format!("unsupported grid placement {s:?}")),
    }
}

/// `gridArea: "nav"` → both axes span the named area.
pub fn parse_area_placement(name: &str) -> Line<GridPlacement<String>> {
    Line {
        start: GridPlacement::NamedLine(name.to_string(), 0),
        end: GridPlacement::NamedLine(name.to_string(), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_tracks() {
        let t = parse_tracks("minmax(180px, 1fr) 2fr auto 120 repeat(2, 1fr 40px)").unwrap();
        assert_eq!(t.len(), 5);
        assert!(matches!(t[4], GridTemplateComponent::Repeat(_)));
    }

    #[test]
    fn parses_areas() {
        let a = parse_areas(&["nav main".into(), "nav footer".into()]).unwrap();
        let nav = a.iter().find(|x| x.name == "nav").unwrap();
        assert_eq!((nav.row_start, nav.row_end, nav.column_start, nav.column_end), (1, 3, 1, 2));
        let footer = a.iter().find(|x| x.name == "footer").unwrap();
        assert_eq!((footer.row_start, footer.row_end, footer.column_start, footer.column_end), (2, 3, 2, 3));
    }

    #[test]
    fn rejects_l_shapes() {
        assert!(parse_areas(&["a a".into(), "a b".into()]).is_err());
    }

    #[test]
    fn parses_placements() {
        assert!(matches!(parse_placement("1 / 3").unwrap().end, GridPlacement::Line(_)));
        assert!(matches!(parse_placement("span 2").unwrap().start, GridPlacement::Span(2)));
        assert!(matches!(parse_placement("nav").unwrap().end, GridPlacement::NamedLine(_, 0)));
    }
}
