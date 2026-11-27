//! #EXP-2204: `fixtures/ci-log.json` + `fixtures/job-graph.json` (the TS
//! reference's `src/ci-log.ts` + `src/job-graph.ts`): the status table, line
//! anchors, the log model, rows, search, reveal, blame and the JobGraph
//! layout + upstream sets.

use eui_core::ci_log::{blame_blocks, ci_status_entry, extend_line_anchor, format_line_anchor, log_model, log_open_default, log_reveal, log_rows, log_search, parse_line_anchor_value, CI_STATUS_SPIN_MS};
use eui_core::job_graph::{job_graph_layout, job_graph_upstream, JobGraphGeometry, JOB_GRAPH};
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/{name}")).expect("fixture")).expect("json")
}

#[test]
fn the_status_table() {
    let f = fixture("ci-log.json");
    assert_eq!(f["statusTable"]["spinMs"], json!(CI_STATUS_SPIN_MS));
    for row in f["statusTable"]["rows"].as_array().unwrap() {
        let mut want = row.clone();
        want.as_object_mut().unwrap().remove("input");
        assert_eq!(ci_status_entry(row.get("input")).to_json(), want, "{row}");
    }
}

#[test]
fn anchors_parse_format_extend() {
    let f = fixture("ci-log.json");
    let a = &f["anchors"];
    assert!(a["parse"].as_array().unwrap().len() > 20);
    for c in a["parse"].as_array().unwrap() {
        let got = parse_line_anchor_value(c.get("text")).map(|r| json!({"start": r.start, "end": r.end})).unwrap_or(Value::Null);
        assert_eq!(got, c["range"], "{c}");
    }
    for c in a["format"].as_array().unwrap() {
        let start = c["start"].as_u64().unwrap();
        let end = c.get("end").and_then(Value::as_u64).unwrap_or(start);
        assert_eq!(format_line_anchor(start, end), c["anchor"].as_str().unwrap(), "{c}");
    }
    for c in a["extend"].as_array().unwrap() {
        let got = extend_line_anchor(c.get("current"), c["line"].as_u64().unwrap(), c["shift"].as_bool().unwrap());
        assert_eq!(got, c["anchor"].as_str().unwrap(), "{c}");
    }
}

#[test]
fn logs_model_open_rows_search_reveal() {
    let f = fixture("ci-log.json");
    let logs = f["logs"].as_array().unwrap();
    assert!(logs.len() > 5);
    for c in logs {
        let name = &c["name"];
        let model = log_model(c.get("lines"), c.get("sections"));
        assert_eq!(model.to_json(), c["model"], "{name}");
        assert_eq!(json!(log_open_default(&model)), c["openDefault"], "{name}");
        for r in c["rows"].as_array().unwrap() {
            let rows: Vec<Value> = log_rows(&model, r.get("open")).iter().map(|r| r.to_json()).collect();
            assert_eq!(Value::Array(rows), r["rows"], "{name} rows {}", r["open"]);
        }
        for s in c["search"].as_array().unwrap() {
            let q = s["query"].as_str().unwrap_or("");
            let got: Vec<Value> = log_search(&model, q).iter().map(|m| m.to_json()).collect();
            assert_eq!(Value::Array(got), s["matches"], "{name} search {q}");
        }
        for r in c["reveal"].as_array().unwrap() {
            assert_eq!(json!(log_reveal(&model, r.get("open"), r["line"].as_u64().unwrap() as usize)), r["result"], "{name} reveal {}", r["line"]);
        }
    }
}

#[test]
fn blame_blocks_like_the_reference() {
    let f = fixture("ci-log.json");
    for c in f["blame"].as_array().unwrap() {
        let got: Vec<Value> = blame_blocks(c.get("blame"), c["lineCount"].as_u64().unwrap() as usize).iter().map(|b| b.to_json()).collect();
        assert_eq!(Value::Array(got), c["lines"], "{}", c["name"]);
    }
}

#[test]
fn job_graph_layout_like_the_reference() {
    let f = fixture("job-graph.json");
    let g = &f["geometry"];
    let n = |k: &str| g[k].as_f64().unwrap();
    let geometry = JobGraphGeometry { node_width: n("nodeWidth"), node_height: n("nodeHeight"), column_gap: n("columnGap"), row_gap: n("rowGap"), stage_height: n("stageHeight"), stage_gap: n("stageGap"), edge_width: n("edgeWidth"), row_height: n("rowHeight") };
    assert_eq!(geometry, JOB_GRAPH);
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() > 5);
    for c in cases {
        let name = &c["name"];
        let graph = job_graph_layout(c.get("jobs"), false, &JOB_GRAPH);
        assert_eq!(graph.to_json(), c["graph"], "{name} graph");
        assert_eq!(job_graph_layout(c.get("jobs"), true, &JOB_GRAPH).to_json(), c["list"], "{name} list");
        for u in c["upstream"].as_array().unwrap() {
            assert_eq!(json!(job_graph_upstream(&graph, u["id"].as_str().unwrap())), u["ids"], "{name} upstream {}", u["id"]);
        }
    }
}

#[test]
fn style_json_carries_the_same_geometry() {
    let style: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/catalog/style.json")).unwrap()).unwrap();
    let j = &style["jobGraph"];
    assert_eq!(j["nodeWidth"].as_f64(), Some(JOB_GRAPH.node_width));
    assert_eq!(j["nodeHeight"].as_f64(), Some(JOB_GRAPH.node_height));
    assert_eq!(j["columnGap"].as_f64(), Some(JOB_GRAPH.column_gap));
    assert_eq!(j["rowGap"].as_f64(), Some(JOB_GRAPH.row_gap));
    assert_eq!(j["stageHeight"].as_f64(), Some(JOB_GRAPH.stage_height));
    assert_eq!(j["stageGap"].as_f64(), Some(JOB_GRAPH.stage_gap));
    assert_eq!(j["edgeWidth"].as_f64(), Some(JOB_GRAPH.edge_width));
    assert_eq!(j["rowHeight"].as_f64(), Some(JOB_GRAPH.row_height));
    assert_eq!(style["ciStatus"]["spinMs"], json!(CI_STATUS_SPIN_MS));
}
