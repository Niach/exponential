//! A job's containment around ecode's [`ProcessSandbox`]:
//!
//! - **Resource scope** (Linux): every step runs in its own transient systemd
//!   user scope (`systemd-run --user --scope`): `MemoryMax` = the runner's
//!   `memoryMaxMb`, `CPUWeight=1`, in the caller's slice. The step's process
//!   itself runs at SCHED_IDLE and nice 19. Without a user manager (a system
//!   service, a container) the step runs without the scope, idle still.
//! - **Namespace sandbox** (Linux, #EXP-2181): the step re-enters the host's
//!   own executable as `__ci-sandbox` ([`sandbox_main`]), which unshares a
//!   user, mount and pid namespace, lays an empty tmpfs over every hidden
//!   directory (the app's data directory, the agents' sign-ins, the user's
//!   runtime directory with its D-Bus and systemd sockets) and `/dev/null`
//!   over every hidden file, mounts a fresh `/proc` (no `/proc/<app>/root`
//!   back door) and only then runs the step. A host that requires it refuses
//!   the job when the namespaces are unavailable ([`hiding_available`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use ecode::{JobSpec, ProcessSandbox, Remote, Sandbox};

/// The argv[1] the host executable dispatches to [`sandbox_main`].
pub const SANDBOX_ARG: &str = "__ci-sandbox";

/// What a job must not see and how its steps are capped.
#[derive(Clone, Debug, Default)]
pub struct Containment {
    /// Directories that read as empty inside a step.
    pub hide_dirs: Vec<PathBuf>,
    /// Files that read as empty inside a step.
    pub hide_files: Vec<PathBuf>,
    /// The executable re-entered with [`SANDBOX_ARG`]; `None` = no namespace
    /// sandbox (a server whose runner settings allow it).
    pub helper: Option<PathBuf>,
    /// The step scope's `MemoryMax`, in MiB.
    pub memory_max_mb: Option<u64>,
}

impl Containment {
    /// Whether the job's own directories stay visible: a hidden directory
    /// must not hold (or be) the runner's work root.
    pub fn check_work_root(&self, work_root: &Path) -> Result<(), String> {
        for dir in &self.hide_dirs {
            if work_root.starts_with(dir) {
                return Err(format!("the CI work directory {} is inside a hidden directory {}", work_root.display(), dir.display()));
            }
        }
        Ok(())
    }
}

/// How `systemd-run` may wrap a step here (probed once per process).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeMode {
    /// `--slice-inherit` (systemd ≥ 252): the scope stays in the caller's
    /// slice, so a slice's own caps (mint's `expbg.slice`) still hold.
    Inherit,
    /// An older systemd: the user manager's default slice.
    Plain,
    None,
}

fn scope_args(mode: ScopeMode, memory_max_mb: Option<u64>) -> Vec<OsString> {
    if mode == ScopeMode::None {
        return vec![];
    }
    let mut args: Vec<OsString> =
        ["systemd-run", "--user", "--scope", "--quiet", "--collect", "--description=Exponential CI step"].map(OsString::from).to_vec();
    if mode == ScopeMode::Inherit {
        args.push("--slice-inherit".into());
    }
    args.push("-p".into());
    args.push("CPUWeight=1".into());
    if let Some(mb) = memory_max_mb.filter(|mb| *mb > 0) {
        args.push("-p".into());
        args.push(format!("MemoryMax={mb}M").into());
    }
    args.push("--".into());
    args
}

fn scope_mode() -> ScopeMode {
    static MODE: OnceLock<ScopeMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        if !cfg!(target_os = "linux") {
            return ScopeMode::None;
        }
        for mode in [ScopeMode::Inherit, ScopeMode::Plain] {
            let mut argv = scope_args(mode, None);
            argv.push("true".into());
            let ok = Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if ok {
                log::info!("ci runner: steps run in systemd user scopes ({mode:?})");
                return mode;
            }
        }
        log::info!("ci runner: no systemd user manager, steps run without a scope");
        ScopeMode::None
    })
}

fn helper_argv(containment: &Containment) -> Vec<OsString> {
    let Some(helper) = &containment.helper else { return vec![] };
    let mut argv = vec![helper.clone().into_os_string(), SANDBOX_ARG.into()];
    for dir in &containment.hide_dirs {
        argv.push("--hide".into());
        argv.push(dir.clone().into_os_string());
    }
    for file in &containment.hide_files {
        argv.push("--hide-file".into());
        argv.push(file.clone().into_os_string());
    }
    argv.push("--".into());
    argv
}

/// Whether `containment`'s namespace sandbox works on this machine: the
/// helper hides a probe directory and the probe reads it empty. `false` on
/// every OS but Linux, and where unprivileged user namespaces are off.
pub fn hiding_available(containment: &Containment, scratch: &Path) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("only Linux can hide the app's data from a CI job".to_string());
    }
    if containment.helper.is_none() {
        return Err("no sandbox helper".to_string());
    }
    let probe = scratch.join("hide-probe");
    std::fs::create_dir_all(&probe).map_err(|e| format!("probe dir: {e}"))?;
    let marker = probe.join("visible");
    std::fs::write(&marker, b"1").map_err(|e| format!("probe file: {e}"))?;
    let only_probe = Containment { hide_dirs: vec![probe.clone()], hide_files: vec![], ..containment.clone() };
    let mut argv = helper_argv(&only_probe);
    argv.extend(["sh", "-c", "test ! -e \"$1\"", "probe"].map(OsString::from));
    argv.push(marker.into_os_string());
    let out = Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::null()).output().map_err(|e| format!("sandbox helper: {e}"))?;
    let _ = std::fs::remove_dir_all(&probe);
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { "user namespaces are unavailable".to_string() } else { err })
    }
}

/// ecode's process sandbox inside the scope and the namespace sandbox.
pub struct JobSandbox {
    inner: ProcessSandbox,
    containment: Containment,
    scope: ScopeMode,
}

impl JobSandbox {
    pub fn new(work_root: impl Into<PathBuf>, containment: Containment) -> Self {
        let scope = scope_mode();
        let mut inner = ProcessSandbox::new(work_root);
        // `systemd-run --user` finds the user manager through it; inside the
        // namespace sandbox it names an empty directory.
        if scope != ScopeMode::None {
            inner = inner.with_passthrough(["XDG_RUNTIME_DIR".to_string()]);
        }
        Self { inner, containment, scope }
    }

    /// The step's argv: scope, then helper, then the shell.
    fn wrap(&self, base: &Command) -> Vec<OsString> {
        let mut argv = scope_args(self.scope, self.containment.memory_max_mb);
        argv.extend(helper_argv(&self.containment));
        argv.push(base.get_program().to_owned());
        argv.extend(base.get_args().map(|arg| arg.to_owned()));
        argv
    }
}

impl Sandbox for JobSandbox {
    fn prepare(&self, job: &JobSpec, remote: &dyn Remote) -> ecode::Result<PathBuf> {
        self.inner.prepare(job, remote)
    }

    fn shell(&self, cwd: &Path, script: &str) -> Command {
        let base = self.inner.shell(cwd, script);
        let argv = self.wrap(&base);
        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]).current_dir(cwd);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: async-signal-safe syscalls only (sched_setscheduler,
            // setpriority) between fork and exec; no allocation. Inherited by
            // systemd-run, the helper and the step.
            unsafe {
                command.pre_exec(|| {
                    let param = libc::sched_param { sched_priority: 0 };
                    libc::sched_setscheduler(0, libc::SCHED_IDLE, &param);
                    libc::setpriority(libc::PRIO_PROCESS, 0, 19);
                    Ok(())
                });
            }
        }
        command
    }

    fn passthrough(&self) -> &[String] {
        self.inner.passthrough()
    }

    fn release(&self, job: &JobSpec, workspace: &Path) {
        self.inner.release(job, workspace)
    }
}

/// The host's `main` calls this first: `Some(exit code)` when this process is
/// the sandbox helper (`argv[1] == __ci-sandbox`), which never returns
/// otherwise; `None` for every other start.
pub fn sandbox_main() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(SANDBOX_ARG.as_ref()) {
        return None;
    }
    let mut hide_dirs = vec![];
    let mut hide_files = vec![];
    let mut command: Vec<OsString> = vec![];
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--hide") => hide_dirs.extend(args.next().map(PathBuf::from)),
            Some("--hide-file") => hide_files.extend(args.next().map(PathBuf::from)),
            Some("--") => {
                command.extend(args.by_ref());
                break;
            }
            _ => {
                eprintln!("{SANDBOX_ARG}: unexpected argument {arg:?}");
                return Some(125);
            }
        }
    }
    if command.is_empty() {
        eprintln!("{SANDBOX_ARG}: no command");
        return Some(125);
    }
    Some(run_sandboxed(&hide_dirs, &hide_files, &command))
}

#[cfg(not(target_os = "linux"))]
fn run_sandboxed(_: &[PathBuf], _: &[PathBuf], _: &[OsString]) -> i32 {
    eprintln!("{SANDBOX_ARG}: only Linux can hide the app's data from a CI job");
    125
}

#[cfg(target_os = "linux")]
fn run_sandboxed(hide_dirs: &[PathBuf], hide_files: &[PathBuf], command: &[OsString]) -> i32 {
    match linux::enter(hide_dirs, hide_files).and_then(|()| linux::fork_and_exec(command)) {
        Ok(code) => code,
        Err(err) => {
            // Fail closed: a step never runs half-hidden.
            eprintln!("{SANDBOX_ARG}: cannot hide the app's data from this job: {err}");
            125
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{CString, OsString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    fn cstr(path: &Path) -> Result<CString, String> {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| format!("bad path {}", path.display()))
    }

    fn os_err(what: &str) -> String {
        format!("{what}: {}", std::io::Error::last_os_error())
    }

    /// Mounts `source` on `target` (both C strings, `None` = NULL).
    fn mount(source: &str, target: &CString, fstype: Option<&str>, flags: libc::c_ulong, data: Option<&str>) -> Result<(), String> {
        let source = CString::new(source).unwrap_or_default();
        let fstype = fstype.map(|t| CString::new(t).unwrap_or_default());
        let data = data.map(|d| CString::new(d).unwrap_or_default());
        // SAFETY: valid NUL-terminated strings for the call's duration.
        let rc = unsafe {
            libc::mount(
                source.as_ptr(),
                target.as_ptr(),
                fstype.as_ref().map_or(std::ptr::null(), |t| t.as_ptr()),
                flags,
                data.as_ref().map_or(std::ptr::null(), |d| d.as_ptr().cast()),
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(os_err(&format!("mount {}", target.to_string_lossy())))
        }
    }

    /// New user + mount + pid namespaces (this process keeps its own uid and
    /// gid), the hidden paths covered.
    pub fn enter(hide_dirs: &[PathBuf], hide_files: &[PathBuf]) -> Result<(), String> {
        // SAFETY: plain syscalls on this process.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        // The step dies with the runner thread that started it (a crash or a
        // kill of the app takes its jobs along; the server re-queues them).
        // SAFETY: prctl on this process.
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
        // SAFETY: unshare(2) on this single-threaded process.
        if unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS | libc::CLONE_NEWPID) } != 0 {
            return Err(os_err("unshare"));
        }
        std::fs::write("/proc/self/setgroups", "deny").map_err(|e| format!("setgroups: {e}"))?;
        std::fs::write("/proc/self/uid_map", format!("{uid} {uid} 1\n")).map_err(|e| format!("uid_map: {e}"))?;
        std::fs::write("/proc/self/gid_map", format!("{gid} {gid} 1\n")).map_err(|e| format!("gid_map: {e}"))?;
        let root = CString::new("/").unwrap_or_default();
        mount("none", &root, None, libc::MS_REC | libc::MS_PRIVATE, None)?;
        for dir in hide_dirs.iter().filter(|dir| dir.is_dir()) {
            mount("tmpfs", &cstr(dir)?, Some("tmpfs"), libc::MS_NOSUID | libc::MS_NODEV, Some("size=1m,mode=0700"))?;
        }
        for file in hide_files.iter().filter(|file| file.exists()) {
            let target = cstr(file)?;
            if file.is_dir() {
                mount("tmpfs", &target, Some("tmpfs"), libc::MS_NOSUID | libc::MS_NODEV, Some("size=1m,mode=0700"))?;
            } else {
                mount("/dev/null", &target, None, libc::MS_BIND, None)?;
            }
        }
        Ok(())
    }

    /// The step as pid 1 of the new pid namespace, under a fresh `/proc`;
    /// this process waits for it and exits as it did.
    pub fn fork_and_exec(command: &[OsString]) -> Result<i32, String> {
        let argv: Vec<CString> = command
            .iter()
            .map(|arg| CString::new(arg.as_bytes()).map_err(|_| "a NUL in the command".to_string()))
            .collect::<Result<_, _>>()?;
        let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|arg| arg.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let proc_dir = CString::new("/proc").unwrap_or_default();
        let proc_src = CString::new("proc").unwrap_or_default();
        // SAFETY: fork(2) in the helper process (single-threaded: it was
        // just exec'd); the child uses only async-signal-safe calls before
        // execvp, on memory prepared above.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(os_err("fork"));
        }
        if pid == 0 {
            unsafe {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                let flags = libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC;
                if libc::mount(proc_src.as_ptr(), proc_dir.as_ptr(), proc_src.as_ptr(), flags, std::ptr::null()) != 0 {
                    let msg = b"__ci-sandbox: cannot mount a private /proc\n";
                    libc::write(2, msg.as_ptr().cast(), msg.len());
                    libc::_exit(125);
                }
                libc::execvp(ptrs[0], ptrs.as_ptr());
                let msg = b"__ci-sandbox: cannot start the step\n";
                libc::write(2, msg.as_ptr().cast(), msg.len());
                libc::_exit(127);
            }
        }
        let mut status = 0;
        loop {
            // SAFETY: waits for the child forked above.
            let rc = unsafe { libc::waitpid(pid, &mut status, 0) };
            if rc == pid {
                break;
            }
            if rc < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                return Err(os_err("waitpid"));
            }
        }
        Ok(if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else if libc::WIFSIGNALED(status) {
            128 + libc::WTERMSIG(status)
        } else {
            1
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_args_cap_memory_and_cpu_in_the_callers_slice() {
        let args: Vec<String> = scope_args(ScopeMode::Inherit, Some(4096)).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(&args[..3], ["systemd-run", "--user", "--scope"]);
        assert!(args.contains(&"--slice-inherit".to_string()));
        assert!(args.contains(&"CPUWeight=1".to_string()));
        assert!(args.contains(&"MemoryMax=4096M".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("--"));
        assert!(scope_args(ScopeMode::None, Some(4096)).is_empty());
        let uncapped: Vec<String> = scope_args(ScopeMode::Plain, None).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(!uncapped.iter().any(|a| a.starts_with("MemoryMax")));
        assert!(!uncapped.contains(&"--slice-inherit".to_string()));
    }

    #[test]
    fn a_work_root_inside_a_hidden_dir_is_refused() {
        let c = Containment { hide_dirs: vec![PathBuf::from("/home/u/.local/share/exponential")], ..Default::default() };
        assert!(c.check_work_root(Path::new("/home/u/.local/share/exponential/ci")).is_err());
        assert!(c.check_work_root(Path::new("/home/u/.cache/exponential-ci")).is_ok());
    }

    #[test]
    fn helper_argv_names_every_hidden_path_before_the_command() {
        let c = Containment {
            hide_dirs: vec![PathBuf::from("/d")],
            hide_files: vec![PathBuf::from("/f")],
            helper: Some(PathBuf::from("/bin/app")),
            memory_max_mb: None,
        };
        let argv: Vec<String> = helper_argv(&c).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(argv, ["/bin/app", SANDBOX_ARG, "--hide", "/d", "--hide-file", "/f", "--"]);
        assert!(helper_argv(&Containment::default()).is_empty());
    }
}
