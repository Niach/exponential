//! #EXP-1617: GitHub workflow YAML TEXT reads on a device exactly as on the
//! server: every `text` case of packages/ecode/fixtures/workflow.json (blessed
//! by the TS reference, which parses with `yaml`) through yaml-rust2.

use ecode::{read_workflow_text, read_workflows};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../../packages/ecode/fixtures/workflow.json");

#[test]
fn text_cases_read_as_the_server_reads_them() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let cases = fixture["text"].as_array().expect("workflow.json has text cases");
    assert!(!cases.is_empty());
    for case in cases {
        let read = read_workflow_text(case["yaml"].as_str().unwrap());
        assert_eq!(read.file.unwrap_or(Value::Null), case["file"], "file: {}", case["name"]);
        assert_eq!(serde_json::to_value(read.errors).unwrap(), case["errors"], "errors: {}", case["name"]);
    }
}

#[test]
fn reads_a_checkouts_workflow_dir() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("ecode-workflows-{}-{n}", std::process::id()));
    let dir = root.join(".github/workflows");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ci.yml"), "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: make\n").unwrap();
    std::fs::write(dir.join("broken.yaml"), "on: [push\n").unwrap();
    std::fs::write(dir.join("README.md"), "not a workflow").unwrap();
    let files = read_workflows(&root).unwrap();
    assert_eq!(files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["broken.yaml", "ci.yml"]);
    assert_eq!(files[0].read.errors[0].code, "yaml");
    assert!(files[1].read.file.is_some());
    assert!(read_workflows(&root.join("nope")).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
