//! #EDE-57: the uploader's half of fixtures/team-import.json, replayed.

use elocal::share::{chunk_rows, validate_chunk, ChunkCaps, ImportRow, TEAM_IMPORT_JSON};
use serde_json::{Map, Value};

fn rows_of(v: &Value) -> Vec<ImportRow> {
    v.as_array().unwrap().iter().map(|r| ImportRow { table: r["table"].as_str().unwrap().into(), id: r["id"].as_str().unwrap().into(), row: r["row"].as_object().cloned().unwrap() }).collect()
}

#[test]
fn chunking() {
    let f: Value = serde_json::from_str(TEAM_IMPORT_JSON).unwrap();
    for c in f["chunking"]["cases"].as_array().unwrap() {
        let caps = ChunkCaps { max_rows: c["maxRows"].as_u64().unwrap() as usize, max_bytes: c["maxBytes"].as_u64().unwrap() as usize };
        let got: Vec<Vec<String>> = chunk_rows(&rows_of(&c["rows"]), &caps).into_iter().map(|chunk| chunk.iter().map(|r| format!("{}/{}", r.table, r.id)).collect()).collect();
        let want: Vec<Vec<String>> = c["expected"].as_array().unwrap().iter().map(|chunk| chunk.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()).collect();
        assert_eq!(got, want, "{}", c["name"]);
    }
}

#[test]
fn validation() {
    let f: Value = serde_json::from_str(TEAM_IMPORT_JSON).unwrap();
    for c in f["validateChunk"]["cases"].as_array().unwrap() {
        let rows: Vec<ImportRow> = match c.get("rowsRepeat") {
            Some(repeat) => {
                let times = repeat["times"].as_u64().unwrap() as usize;
                let body = repeat.get("bodyBytes").and_then(Value::as_u64).map(|n| "x".repeat(n as usize));
                (0..times)
                    .map(|i| {
                        let mut row = Map::new();
                        row.insert("id".into(), Value::from(format!("r{i}")));
                        if let Some(body) = &body {
                            row.insert("body".into(), Value::from(body.clone()));
                        }
                        ImportRow { table: repeat["table"].as_str().unwrap().into(), id: format!("r{i}"), row }
                    })
                    .collect()
            }
            None => rows_of(&c["rows"]),
        };
        assert_eq!(validate_chunk(&rows), c["error"].as_str().map(str::to_string), "{}", c["name"]);
    }
}
