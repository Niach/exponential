//! eApps §15: the local records query over 25 000 rows (the team plan's
//! quota), p50/p95 per query shape. `cargo run --release -p eui-core
//! --example records_query_25k`. Budget: p95 ≤ 16 ms on a mid phone.

use eui_core::host::{parse_records_json, parse_records_query, records_value_owned, RecordRow, RecordsContext, RecordsQuery};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

fn main() {
    let authors = ["u1", "u2", "u3", "u4", "u5"];
    let rows: Vec<RecordRow> = (0..25_000)
        .map(|i| {
            let day = chrono_day(i % 330);
            let mut data = json!({"date": day, "hours": (i % 9) as f64 + 0.5, "project": format!("p{}", i % 7)});
            if i % 3 != 0 {
                data["note"] = json!(format!("n{i}"));
            }
            RecordRow { id: format!("r{i}"), kind: "entry".into(), data, created_by: Some(authors[i % 5].into()), author_id: Some(authors[i % 5].into()), created_at: "2026-01-01T00:00:00.000Z".into(), updated_at: "2026-01-01T00:00:00.000Z".into() }
        })
        .collect();
    let url = |pairs: &[(&str, &str)]| -> RecordsQuery {
        let params: BTreeMap<String, String> = pairs.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
        parse_records_query(&params, Some("entry"))
    };
    let queries: Vec<(&str, RecordsQuery)> = vec![
        ("month view mine sum", url(&[("mine", "1"), ("by", "date"), ("sum", "hours"), ("month", "2026-10")])),
        ("filter + sort + limit 500", url(&[("hours.gte", "4"), ("sort", "date"), ("order", "desc"), ("limit", "500")])),
        ("group author x month bucket, sum+count", parse_records_json(&json!({"from": "entry", "group": ["author", {"bucket": "date", "by": "month"}], "aggregate": {"h": ["sum", "hours"], "n": ["count"]}}), Some("entry")).0),
        ("group project, running sum", parse_records_json(&json!({"from": "entry", "group": ["project"], "aggregate": {"h": ["sum", "hours"]}, "running": {"h": "sum"}}), Some("entry")).0),
        ("between + in", parse_records_json(&json!({"from": "entry", "where": [["date", "between", "2026-03-01", "2026-06-30"], ["project", "in", ["p1", "p2"]]], "limit": 500}), Some("entry")).0),
    ];
    let names = HashMap::new();
    for (name, q) in &queries {
        let run = || records_value_owned(&rows, q, RecordsContext { me: Some("u1"), now: "2026-11-02T09:30:00.000Z", today: None, names: &names, computed: None, refs: None, issues: None });
        for _ in 0..5 {
            run();
        }
        let mut ts: Vec<f64> = (0..60)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(run());
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("{name:<42} p50 {:.1} ms  p95 {:.1} ms", ts[30], ts[57]);
    }
}

/// `2026-01-01` + n days as `YYYY-MM-DD`.
fn chrono_day(n: usize) -> String {
    let (mut y, mut m, mut d) = (2026, 1, 1 + n as u32);
    let len = |m: u32| [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize];
    while d > len(m) {
        d -= len(m);
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}
