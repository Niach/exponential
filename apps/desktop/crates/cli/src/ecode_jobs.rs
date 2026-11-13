//! #EXP-1616: eCode's git jobs on this machine (`device_commands` kind
//! `ecode_job`, cap `ecode-jobs`). The server's own forge runs nothing that
//! needs git: mergeability, squash merges, `rebase --onto`, file lists,
//! bundle backups, restores, repacks and mirror pushes come here and run
//! with `ecode::LocalRepo` on a bare cache of the repo, filled from the team
//! bucket (`ecode::exp_jobs`). The payload carries a short-lived job token;
//! the answer is the JSON the server's `jobs.ts` parses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

/// The cap that makes the server pick this machine for jobs.
pub const ECODE_JOBS_CAP: &str = "ecode-jobs";

/// One job at a time per repo cache (two jobs on one bare repo would race
/// its refs); different repos run in parallel.
fn repo_lock(repository_id: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    locks.entry(repository_id.to_string()).or_default().clone()
}

/// The bare cache of one repo under the data dir.
pub fn cache_dir(data_dir: &Path, repository_id: &str) -> PathBuf {
    data_dir.join("ecode").join("repos").join(format!("{repository_id}.git"))
}

/// Runs one job payload (`{repositoryId, op, args, token}`) against
/// `server`; `(ok, answer JSON)`.
pub fn run(server: &str, data_dir: &Path, payload: &Value) -> (bool, String) {
    let field = |name: &str| payload[name].as_str().unwrap_or_default();
    let (repository_id, token) = (field("repositoryId"), field("token"));
    let args: Value = serde_json::from_str(field("args")).unwrap_or(Value::Null);
    if repository_id.is_empty() || token.is_empty() || !args.is_object() || repository_id.contains(['/', '\\', '.']) {
        return (false, serde_json::json!({ "ok": false, "message": "Malformed eCode job" }).to_string());
    }
    let lock = repo_lock(repository_id);
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
    let outcome = ecode::exp_git::ExpGit::new(server, token)
        .and_then(|git| ecode::exp_jobs::run_job(&git, &cache_dir(data_dir, repository_id), &args));
    ecode::exp_jobs::job_answer(outcome)
}
