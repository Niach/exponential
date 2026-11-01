//! EXP-1601: the spike's contract checks: a sampled frame is the flat
//! display list, expressions are refused, the packed stream is versioned.

use eui_motion::display::{code, Brush, Op};
use eui_motion::{DisplayList, Motion};

const RECT: &str = r#"{"v":"5.7.0","fr":30,"ip":0,"op":30,"w":100,"h":50,"layers":[
 {"ty":4,"ind":1,"ip":0,"op":30,"st":0,"ks":{"o":{"a":0,"k":100},"p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0}},
  "shapes":[{"ty":"rc","p":{"a":0,"k":[50,25]},"s":{"a":0,"k":[20,10]},"r":{"a":0,"k":0}},
            {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]}]}"#;

#[test]
fn samples_a_rect_into_clip_fill_pop() {
    let motion = Motion::load(RECT.as_bytes()).expect("loads");
    assert_eq!(motion.size(), (100.0, 50.0));
    assert_eq!(motion.duration(), 1.0);
    let mut list = DisplayList::default();
    motion.sample(0.0, &mut list);
    assert!(matches!(list.ops.first(), Some(Op::Clip { .. })), "{:?}", list.ops);
    assert!(matches!(list.ops.last(), Some(Op::Pop)));
    let fill = list
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Fill { brush, .. } => Some(brush.clone()),
            _ => None,
        })
        .expect("a fill");
    assert_eq!(fill, Brush::Solid { color: [1.0, 0.0, 0.0, 1.0] });
}

#[test]
fn refuses_expressions() {
    let src = RECT.replace(r#""r":{"a":0,"k":0}},"#, r#""r":{"a":0,"k":0,"x":"time*10"}},"#);
    let issues = Motion::load(src.as_bytes()).err().expect("refused");
    assert!(issues.iter().any(|i| i.code == "expression"), "{issues:?}");
}

#[test]
fn packed_stream_is_versioned() {
    let motion = Motion::load(RECT.as_bytes()).unwrap();
    let mut list = DisplayList::default();
    motion.sample(0.0, &mut list);
    let mut packed = Vec::new();
    list.encode(&mut packed);
    assert_eq!(&packed[..3], &[code::VERSION, 100.0, 50.0]);
    assert_eq!(packed[3], code::CLIP);
}
