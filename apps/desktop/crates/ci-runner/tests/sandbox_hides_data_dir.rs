//! #EXP-2181: a CI job cannot read the app's data directory.
//!
//! A step runs through the same [`JobSandbox`] the runner uses, with a stand-in
//! "data directory" holding a secret, and tries every way we know to reach it:
//! the path itself, a hidden file, and `/proc/<runner pid>/root`, which leads
//! back into the runner's own mount namespace. Where this machine has no
//! unprivileged user namespaces the runner refuses the job instead, and the
//! test proves that: either way the secret never reaches the job.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use ci_runner::sandbox::hiding_available;
use ci_runner::{Containment, JobSandbox, Runner};
use ecode::Sandbox;

const SECRET: &str = "exp-sandbox-secret-7f3a";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exp-ci-sandbox-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn containment(data: &Path, token_file: &Path) -> Containment {
    Containment {
        hide_dirs: vec![data.to_path_buf()],
        hide_files: vec![token_file.to_path_buf()],
        helper: Some(PathBuf::from(env!("CARGO_BIN_EXE_ci-sandbox-helper"))),
        memory_max_mb: Some(512),
    }
}

#[test]
fn a_job_cannot_read_the_apps_data_directory() {
    let root = scratch("hide");
    let data = root.join("data");
    let work = root.join("work");
    std::fs::create_dir_all(data.join("agents/claude")).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(data.join("tokens.json"), SECRET).unwrap();
    std::fs::write(data.join("agents/claude/credentials"), SECRET).unwrap();
    let token_file = root.join("claude.json");
    std::fs::write(&token_file, SECRET).unwrap();
    let containment = containment(&data, &token_file);

    let runner = Runner::new(work.clone(), containment.clone(), true);
    if let Err(reason) = hiding_available(&containment, &work) {
        // No namespaces here: the runner must refuse every job.
        assert!(runner.readiness().is_err(), "the runner would run a job unhidden ({reason})");
        eprintln!("namespaces unavailable here ({reason}); proved the runner refuses jobs");
        let _ = std::fs::remove_dir_all(&root);
        return;
    }
    assert!(runner.readiness().is_ok());

    let sandbox = JobSandbox::new(&work, containment);
    let script = format!(
        "cat {data}/tokens.json {data}/agents/claude/credentials {file} /proc/{pid}/root{data}/tokens.json 2>&1 || true; \
         echo listing; ls -A {data}; echo listed; \
         grep -r {secret} {data} /proc/{pid}/root{data} 2>/dev/null || true; echo done",
        data = data.display(),
        file = token_file.display(),
        pid = std::process::id(),
        secret = SECRET,
    );
    let mut command = sandbox.shell(&work, &script);
    command.env_clear().env("PATH", std::env::var_os("PATH").unwrap_or_default());
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        command.env("XDG_RUNTIME_DIR", runtime);
    }
    let out = command.stdin(Stdio::null()).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&root);

    assert!(text.contains("done"), "the step did not run to its end:\n{text}");
    assert!(!text.contains(SECRET), "the job read the data directory:\n{text}");
    // The hidden directory reads as empty, it does not vanish.
    let listing = text.split("listing").nth(1).and_then(|rest| rest.split("listed").next()).unwrap_or_default();
    assert!(!listing.contains("tokens.json") && !listing.contains("agents"), "the job listed the data directory:\n{text}");
}

#[test]
fn the_same_step_without_the_sandbox_reads_it() {
    // The control: the stand-in secret IS readable to an unhidden step, so
    // the test above fails if the sandbox stops hiding it.
    let root = scratch("control");
    let data = root.join("data");
    let work = root.join("work");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(data.join("tokens.json"), SECRET).unwrap();
    let sandbox = JobSandbox::new(&work, Containment::default());
    let mut command = sandbox.shell(&work, &format!("cat {}/tokens.json", data.display()));
    command.env("PATH", std::env::var_os("PATH").unwrap_or_default());
    let out = command.stdin(Stdio::null()).output().unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(String::from_utf8_lossy(&out.stdout).contains(SECRET));
}
