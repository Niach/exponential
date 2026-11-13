//! Runs `examples/local_flow.rs` (the Rerusted proof, #RUST-9) as a test:
//! no server, a file Store, a real repository.

#[path = "../examples/local_flow.rs"]
mod local_flow;

use ecode::Event;

#[test]
fn a_pr_tree_lands_without_a_server() {
    let dir = std::env::temp_dir().join(format!("ecode-local-flow-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let summary = local_flow::run(&dir).unwrap();
    assert_eq!(summary.merged.iter().map(|m| m.0).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(summary.main, summary.merged[1].1, "main ends on the second squash");
    assert_eq!(summary.log, ["UI (#2)", "API (#1)", "init"]);
    let kinds: Vec<&str> = summary
        .events
        .iter()
        .map(|e| match e {
            Event::Pushed { .. } => "pushed",
            Event::PrOpened { .. } => "opened",
            Event::PrRetargeted { .. } => "retargeted",
            Event::PrMerged { .. } => "merged",
            Event::BranchDeleted { .. } => "deleted",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["pushed", "pushed", "pushed", "pushed", "opened", "opened", "retargeted", "merged", "retargeted", "deleted", "merged", "deleted"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
