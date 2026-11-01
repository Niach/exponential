use crate::display::{Brush, DisplayList, Op};
use crate::Motion;
use serde_json::{json, Value};

/// A square glyph `w` wide (100-unit em), lottie's `chars` shape layout.
fn glyph(ch: &str, w: f64) -> Value {
    json!({
        "ch": ch, "size": 100, "style": "Regular", "fFamily": "Test", "w": w,
        "data": {"shapes": [{"ty": "gr", "it": [
            {"ty": "sh", "ks": {"a": 0, "k": {"c": true, "i": [[0,0],[0,0],[0,0],[0,0]], "o": [[0,0],[0,0],[0,0],[0,0]],
                "v": [[0,0],[w,0],[w,-70],[0,-70]]}}},
            {"ty": "tr", "p": {"a":0,"k":[0,0]}, "a": {"a":0,"k":[0,0]}, "s": {"a":0,"k":[100,100]}, "r": {"a":0,"k":0}, "o": {"a":0,"k":100}}
        ]}]}
    })
}

fn text_doc(extra: Value, animators: Value, with_chars: bool) -> Vec<u8> {
    let mut d = json!({"s": 100, "f": "Test-Regular", "t": "ABCD", "j": 0, "tr": 0, "lh": 120, "ls": 0, "fc": [1, 0, 0]});
    for (k, v) in extra.as_object().unwrap() {
        d[k] = v.clone();
    }
    let mut root = json!({
        "v": "5.7.0", "fr": 30, "ip": 0, "op": 10, "w": 1000, "h": 400,
        "fonts": {"list": [{"fName": "Test-Regular", "fFamily": "Test", "fStyle": "Regular", "ascent": 75}]},
        "layers": [{
            "ty": 5, "ind": 1, "ip": 0, "op": 10, "st": 0, "sr": 1,
            "ks": {"p": {"a": 0, "k": [0, 0]}},
            "t": {"d": {"k": [{"s": d, "t": 0}]}, "p": {}, "m": {"g": 1, "a": {"a": 0, "k": [0, 0]}}, "a": animators}
        }]
    });
    if with_chars {
        root["chars"] = json!(["A", "B", "C", "D", " "].iter().map(|c| glyph(c, 50.0)).collect::<Vec<_>>());
    }
    serde_json::to_vec(&root).unwrap()
}

fn fills(m: &Motion) -> Vec<(f32, f32, [f32; 4])> {
    let mut list = DisplayList::default();
    m.sample(0.0, &mut list);
    list.ops
        .iter()
        .filter_map(|op| match op {
            Op::Fill { transform, brush: Brush::Solid { color }, path, .. } if path.len() > 2 && transform[4] > -1e3 => {
                // Skip the full-surface ops (none for text, but be strict).
                Some((transform[4], transform[5], *color))
            }
            _ => None,
        })
        .collect()
}

fn xs(m: &Motion) -> Vec<f32> {
    fills(m).iter().map(|f| f.0).collect()
}

#[test]
fn justify_left_centre_right() {
    let load = |j: i64| Motion::load(&text_doc(json!({"j": j, "t": "AB"}), json!([]), true)).unwrap();
    assert_eq!(xs(&load(0)), vec![0.0, 50.0]);
    assert_eq!(xs(&load(1)), vec![-100.0, -50.0]);
    assert_eq!(xs(&load(2)), vec![-50.0, 0.0]);
    // Fill = the document colour.
    assert_eq!(fills(&load(0))[0].2, [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn tracking_adds_spacing() {
    // 100 thousandths of a 100 px em = 10 px between letters.
    let m = Motion::load(&text_doc(json!({"t": "ABC", "tr": 100}), json!([]), true)).unwrap();
    assert_eq!(xs(&m), vec![0.0, 60.0, 120.0]);
}

#[test]
fn line_breaks_advance_by_line_height() {
    let m = Motion::load(&text_doc(json!({"t": "A\rB"}), json!([]), true)).unwrap();
    let f = fills(&m);
    assert_eq!(f.len(), 2);
    assert_eq!((f[1].0, f[1].1), (0.0, 121.0)); // lottie-web's +1 on the first break
}

#[test]
fn range_selector_hides_first_half() {
    let animators = json!([{
        "s": {"s": {"a": 0, "k": 0}, "e": {"a": 0, "k": 50}, "o": {"a": 0, "k": 0}, "r": 1, "b": 1, "sh": 1,
              "a": {"a": 0, "k": 100}, "xe": {"a": 0, "k": 0}, "ne": {"a": 0, "k": 0}},
        "a": {"o": {"a": 0, "k": 0}}
    }]);
    let m = Motion::load(&text_doc(json!({}), animators, true)).unwrap();
    // A and B fully transparent (skipped), C and D drawn in place.
    assert_eq!(xs(&m), vec![100.0, 150.0]);
}

#[test]
fn animator_position_and_fill() {
    let animators = json!([{
        "s": {"s": {"a": 0, "k": 0}, "e": {"a": 0, "k": 100}, "r": 1, "b": 1, "sh": 1},
        "a": {"p": {"a": 0, "k": [0, -20, 0]}, "fc": {"a": 0, "k": [0, 0, 1, 1]}}
    }]);
    let m = Motion::load(&text_doc(json!({"t": "AB"}), animators, true)).unwrap();
    let f = fills(&m);
    assert_eq!((f[0].0, f[0].1), (0.0, -20.0));
    assert_eq!(f[1].2, [0.0, 0.0, 1.0, 1.0]);
}

const NUNITO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../packages/exponential-ui/conformance/fonts/Nunito-Regular.ttf"
);

#[test]
fn font_book_glyph_has_outline() {
    let data = std::fs::read(NUNITO).unwrap();
    let mut book = super::FontBook::default();
    book.add("Nunito", data);
    let i = book.find("Nunito-Regular", "Nunito", "Regular").unwrap();
    let face = book.face(i).unwrap();
    let g = face.glyph('A').unwrap();
    let paths = face.outline(g);
    assert!(!paths.is_empty() && paths.iter().all(|p| p.closed && p.v.len() > 2));
    assert!(face.advance(g) > 0.3 && face.advance(g) < 1.0);
    let o = face.glyph('o').unwrap();
    assert_eq!(face.outline(o).len(), 2);
}

#[test]
fn font_book_renders_text_without_chars() {
    let src = text_doc(json!({"t": "Hi o"}), json!([]), false);
    let mut m = Motion::load(&src).unwrap();
    assert!(super::needs_fonts(m.model()));
    assert!(fills(&m).is_empty());
    m.add_font("Nunito", std::fs::read(NUNITO).unwrap());
    assert!(!super::needs_fonts(m.model()));
    let x = xs(&m);
    // H, i, o (the space has no outline).
    assert_eq!(x.len(), 3);
    assert!(x[0] == 0.0 && x[1] > 50.0 && x[2] > x[1]);
}

#[test]
fn boxed_text_wraps_at_spaces() {
    // 50 px letters in a 120 px box: "AB" fits, " CD" wraps.
    let m = Motion::load(&text_doc(json!({"t": "AB CD", "sz": [120, 400], "ps": [10, 20]}), json!([]), true)).unwrap();
    let f = fills(&m);
    let pos: Vec<(f32, f32)> = f.iter().map(|f| (f.0, f.1)).collect();
    // First baseline at ps.y + ascent (75 % of 100).
    assert_eq!(pos, vec![(10.0, 95.0), (60.0, 95.0), (10.0, 216.0), (60.0, 216.0)]);
}
