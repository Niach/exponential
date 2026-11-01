//! Host-registered fonts (`Motion::add_font`): cmap, hmtx advances, kerning
//! (`kern` table, else GPOS pair adjustment of the `kern` feature) and glyph
//! outlines as [`BPath`]s (quadratics raised to cubics), via `ttf-parser`.

use crate::geom::{BPath, Pt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ttf_parser::{name_id, Face, GlyphId};

/// Fonts registered by the host, matched by family (and style) name.
#[derive(Default)]
pub struct FontBook {
    fonts: Vec<FontFile>,
    /// Outlines by (font, glyph), in font units (y up).
    outlines: Mutex<HashMap<(usize, u16), Arc<Vec<BPath>>>>,
}

struct FontFile {
    /// The family the host registered it as.
    family: String,
    /// Names from the file's `name` table (lower-cased).
    file_family: String,
    file_style: String,
    postscript: String,
    data: Vec<u8>,
    units_per_em: f64,
    ascender: f64,
}

impl std::fmt::Debug for FontBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.fonts.iter().map(|x| (&x.family, &x.file_style)))
            .finish()
    }
}

fn name(face: &Face<'_>, ids: &[u16]) -> String {
    for id in ids {
        if let Some(s) = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == *id && n.is_unicode())
            .find_map(|n| n.to_string())
        {
            return s.to_lowercase();
        }
    }
    String::new()
}

impl FontBook {
    /// Register a TrueType/OpenType font (`data` = the file bytes) as `family`.
    /// Unparsable data is ignored.
    pub fn add(&mut self, family: &str, data: Vec<u8>) {
        let Ok(face) = Face::parse(&data, 0) else {
            return;
        };
        let units_per_em = face.units_per_em().max(1) as f64;
        let ascender = face.typographic_ascender().unwrap_or_else(|| face.ascender()) as f64;
        let file_family = name(&face, &[name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY]);
        let file_style = name(&face, &[name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY]);
        let postscript = name(&face, &[name_id::POST_SCRIPT_NAME]);
        drop(face);
        self.fonts.push(FontFile {
            family: family.to_lowercase(),
            file_family,
            file_style,
            postscript,
            data,
            units_per_em,
            ascender,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// The registered font for a document font: `name` = `fName`, `family` /
    /// `style` = the font list's `fFamily` / `fStyle` (empty when unknown).
    /// Order: exact name (registered or PostScript), family + style, family,
    /// then the first registered font.
    pub fn find(&self, name: &str, family: &str, style: &str) -> Option<usize> {
        if self.fonts.is_empty() {
            return None;
        }
        let (name, family, style) = (name.to_lowercase(), family.to_lowercase(), style.to_lowercase());
        let full = format!("{family} {style}");
        let fam = |f: &FontFile| !family.is_empty() && (f.family == family || f.file_family == family);
        let by = |pred: &dyn Fn(&FontFile) -> bool| self.fonts.iter().position(pred);
        by(&|f| !name.is_empty() && (f.family == name || f.postscript == name))
            .or_else(|| by(&|f| f.family == full))
            .or_else(|| by(&|f| fam(f) && f.file_style == style))
            .or_else(|| by(&|f| fam(f)))
            .or(Some(0))
    }

    pub(crate) fn face(&self, index: usize) -> Option<FontFace<'_>> {
        let file = self.fonts.get(index)?;
        let face = Face::parse(&file.data, 0).ok()?;
        Some(FontFace { book: self, index, face, units_per_em: file.units_per_em })
    }

    /// Ascent in percent of the em (the font list's `ascent` unit).
    pub(crate) fn ascent_percent(&self, index: usize) -> f64 {
        self.fonts.get(index).map(|f| f.ascender / f.units_per_em * 100.0).unwrap_or(75.0)
    }
}

/// A parsed registered font, alive for one layout.
pub(crate) struct FontFace<'a> {
    book: &'a FontBook,
    index: usize,
    face: Face<'a>,
    pub units_per_em: f64,
}

impl FontFace<'_> {
    pub fn glyph(&self, ch: char) -> Option<GlyphId> {
        self.face.glyph_index(ch)
    }

    /// Advance in ems.
    pub fn advance(&self, g: GlyphId) -> f64 {
        self.face.glyph_hor_advance(g).unwrap_or(0) as f64 / self.units_per_em
    }

    /// Horizontal kerning between two glyphs, in ems.
    pub fn kern(&self, left: GlyphId, right: GlyphId) -> f64 {
        let tables = self.face.tables();
        if let Some(kern) = tables.kern {
            let mut total = 0i32;
            let mut found = false;
            for st in kern.subtables {
                if st.horizontal && !st.variable && !st.has_cross_stream {
                    if let Some(v) = st.glyphs_kerning(left, right) {
                        total += v as i32;
                        found = true;
                    }
                }
            }
            if found {
                return total as f64 / self.units_per_em;
            }
        }
        if let Some(gpos) = tables.gpos {
            return gpos_kern(&gpos, left, right) as f64 / self.units_per_em;
        }
        0.0
    }

    /// Outline in font units (y up), cached.
    pub fn outline(&self, g: GlyphId) -> Arc<Vec<BPath>> {
        let key = (self.index, g.0);
        if let Ok(cache) = self.book.outlines.lock() {
            if let Some(p) = cache.get(&key) {
                return p.clone();
            }
        }
        let mut b = Builder::default();
        self.face.outline_glyph(g, &mut b);
        b.finish();
        let paths = Arc::new(b.paths);
        if let Ok(mut cache) = self.book.outlines.lock() {
            cache.insert(key, paths.clone());
        }
        paths
    }
}

/// GPOS pair adjustment (first glyph's x advance) over every `kern` feature lookup.
fn gpos_kern(gpos: &ttf_parser::opentype_layout::LayoutTable<'_>, left: GlyphId, right: GlyphId) -> i32 {
    use ttf_parser::gpos::{PairAdjustment, PositioningSubtable};
    let kern_tag = ttf_parser::Tag::from_bytes(b"kern");
    let mut seen: Vec<u16> = Vec::new();
    let mut total = 0i32;
    for feature in gpos.features {
        if feature.tag != kern_tag {
            continue;
        }
        for li in feature.lookup_indices {
            if seen.contains(&li) {
                continue;
            }
            seen.push(li);
            let Some(lookup) = gpos.lookups.get(li) else { continue };
            for st in lookup.subtables.into_iter::<PositioningSubtable>() {
                let PositioningSubtable::Pair(pair) = st else { continue };
                let Some(ci) = pair.coverage().get(left) else { continue };
                let rec = match pair {
                    PairAdjustment::Format1 { sets, .. } => sets.get(ci).and_then(|s| s.get(right)),
                    PairAdjustment::Format2 { classes, matrix, .. } => {
                        matrix.get((classes.0.get(left), classes.1.get(right)))
                    }
                };
                if let Some((first, _)) = rec {
                    total += first.x_advance as i32;
                    // The first subtable that covers the pair wins within a lookup.
                    break;
                }
            }
        }
    }
    total
}

#[derive(Default)]
struct Builder {
    paths: Vec<BPath>,
    cur: Option<BPath>,
}

impl Builder {
    fn last(&self) -> Pt {
        self.cur.as_ref().and_then(|p| p.v.last().copied()).unwrap_or([0.0, 0.0])
    }

    fn end(&mut self, closed: bool) {
        if let Some(mut p) = self.cur.take() {
            let n = p.v.len();
            if closed && n > 1 && p.v[n - 1] == p.v[0] {
                // The closing vertex duplicates the first: fold its in-tangent.
                p.i[0] = p.i[n - 1];
                p.v.pop();
                p.i.pop();
                p.o.pop();
            }
            p.closed = closed;
            if !p.v.is_empty() {
                self.paths.push(p);
            }
        }
    }

    fn finish(&mut self) {
        self.end(true);
    }

    fn seg(&mut self, c1: Pt, c2: Pt, p: Pt) {
        if let Some(cur) = self.cur.as_mut() {
            if let Some(o) = cur.o.last_mut() {
                *o = c1;
            }
            cur.push(p, p, c2);
        }
    }
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.end(true);
        let p = [x as f64, y as f64];
        let mut b = BPath::default();
        b.push(p, p, p);
        self.cur = Some(b);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = [x as f64, y as f64];
        let l = self.last();
        self.seg(l, p, p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = self.last();
        let q = [x1 as f64, y1 as f64];
        let p = [x as f64, y as f64];
        let c1 = [p0[0] + 2.0 / 3.0 * (q[0] - p0[0]), p0[1] + 2.0 / 3.0 * (q[1] - p0[1])];
        let c2 = [p[0] + 2.0 / 3.0 * (q[0] - p[0]), p[1] + 2.0 / 3.0 * (q[1] - p[1])];
        self.seg(c1, c2, p);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.seg([x1 as f64, y1 as f64], [x2 as f64, y2 as f64], [x as f64, y as f64]);
    }

    fn close(&mut self) {
        self.end(true);
    }
}
