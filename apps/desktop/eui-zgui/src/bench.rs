//! The bench workloads, byte-identical to the gpui reference's
//! (`gpui/zgui_ref.rs`): the spike's 250-component surface, a 10k-row List
//! of `Row(title, meta)`, and a 10k-row chat-style List of Box > Text with
//! 1–8 wrapped lines each (variable row heights).

use eui_core::types::FlatComponent;
use serde_json::{Value, json};

pub fn surface250() -> (Vec<FlatComponent>, f32, f32) {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(crate::repo().join("apps/desktop/spikes-zgui/surface250.json")).unwrap()).unwrap();
    (serde_json::from_value(v["components"].clone()).unwrap(), v["viewport"]["width"].as_f64().unwrap() as f32, v["viewport"]["height"].as_f64().unwrap() as f32)
}

pub fn list_components(variable: bool) -> Vec<FlatComponent> {
    let row = if variable {
        json!({"id": "bench_row", "component": "Box", "style": {"padding": 12, "borderRadius": 12, "backgroundColor": "$color.muted", "marginBottom": 8}, "children": ["bench_text"]})
    } else {
        json!({"id": "bench_row", "component": "Row", "title": {"@path": "title"}, "meta": {"@path": "meta"}, "pressable": true})
    };
    let mut c = vec![json!({"id": "root", "component": "List", "direction": "vertical", "divided": !variable, "style": {"height": 800}, "children": {"componentId": "bench_row", "path": "/rows", "key": "id"}}), row];
    if variable {
        c.push(json!({"id": "bench_text", "component": "Text", "text": {"@path": "text"}}));
    }
    serde_json::from_value(Value::Array(c)).unwrap()
}

const WORDS: [&str; 16] = ["the", "renderer", "paints", "every", "frame", "from", "eui-core", "a", "chat", "message", "wraps", "onto", "several", "lines", "when", "long"];

pub fn rows(variable: bool) -> Value {
    Value::Array(
        (0..10_000)
            .map(|i: usize| {
                if variable {
                    let lines = 1 + (i * 7919) % 8;
                    let n = lines * 9 - 4;
                    let text: Vec<&str> = (0..n).map(|k| WORDS[(i + k * 3) % WORDS.len()]).collect();
                    json!({"id": format!("r{i}"), "text": format!("#{i} {}", text.join(" "))})
                } else {
                    json!({"id": format!("r{i}"), "title": format!("Row {i}"), "meta": (i % 97).to_string()})
                }
            })
            .collect(),
    )
}
