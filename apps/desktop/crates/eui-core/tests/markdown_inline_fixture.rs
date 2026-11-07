//! Round 9 §9.1: `fixtures/markdown-inline.json` (TS `markdownInlineRecipes`).

use eui_core::recipes::markdown_inline_recipes;
use eui_core::theme::recipe_style;
use eui_core::themes::builtin_theme;
use serde_json::{json, Map, Value};

#[test]
fn inline_recipes_replay_the_fixture() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/markdown-inline.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let neutral = builtin_theme("neutral").unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for c in cases {
        let level = c["level"].as_u64().map(|l| l as u8);
        let recipes = markdown_inline_recipes(c["part"].as_str().unwrap(), c["variant"].as_str(), level);
        let got: Vec<Value> = recipes.iter().map(|q| json!({"component": q.component, "part": q.part, "props": q.props})).collect();
        assert_eq!(json!(got), c["recipes"], "{c}");
        let mut merged = Map::new();
        for q in &recipes {
            merged.extend(recipe_style(&neutral, q).into_iter().map(|(k, v)| (k.to_string(), v)));
        }
        assert_eq!(Value::Object(merged), c["neutral"], "{c}");
    }
}
