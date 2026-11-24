//! The platform's thread stacks, with no privilege the process lacks.
//!
//! - macOS: `/usr/bin/sample <pid> 2` (ships with the system, samples a
//!   process of the same user);
//! - Linux: the task table from `/proc/self/task` (every thread's name,
//!   state, wait channel and system call: needs nothing), then the stacks
//!   from `eu-stack` or `gdb` when one is installed. The tool is this
//!   process's child, which Yama's default (`ptrace_scope` 1) refuses, so
//!   the process names it its tracer first (`PR_SET_PTRACER`, no privilege)
//!   and takes that back after. The file says which of them it got.
//!
//! The child is spawned with `posix_spawn` and a fixed environment:
//! `std::process` takes the environment lock, and this runs while some
//! thread of the process is stuck nobody knows where.

use std::fs::File;
use std::path::Path;

use crate::Sampler;

/// This platform's sampler.
#[derive(Default)]
pub struct PlatformSampler;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl Sampler for PlatformSampler {
    fn sample(&mut self, _pid: u32, _path: &Path, out: &mut File) {
        use std::io::Write as _;
        let _ = writeln!(out, "sampler: none on this platform");
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod spawn {
    use std::ffi::{CStr, CString};
    use std::os::fd::RawFd;
    use std::time::{Duration, Instant};

    /// What the child sees: no inherited variable, no debuginfod download.
    const ENV: [&CStr; 3] = [c"PATH=/usr/bin:/bin:/usr/sbin:/sbin", c"LC_ALL=C", c"DEBUGINFOD_URLS="];

    /// Spawn `argv` with `stdin`, and stdout + stderr on `out`.
    pub fn spawn(argv: &[CString], stdin: RawFd, out: RawFd) -> Option<libc::pid_t> {
        let mut args: Vec<*mut libc::c_char> = argv.iter().map(|arg| arg.as_ptr() as *mut libc::c_char).collect();
        args.push(std::ptr::null_mut());
        let mut env: Vec<*mut libc::c_char> = ENV.iter().map(|var| var.as_ptr() as *mut libc::c_char).collect();
        env.push(std::ptr::null_mut());
        let mut pid: libc::pid_t = 0;
        // SAFETY: every pointer outlives the call; the actions are
        // initialised before use and destroyed after.
        unsafe {
            let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
            if libc::posix_spawn_file_actions_init(&mut actions) != 0 {
                return None;
            }
            libc::posix_spawn_file_actions_adddup2(&mut actions, stdin, 0);
            libc::posix_spawn_file_actions_adddup2(&mut actions, out, 1);
            libc::posix_spawn_file_actions_adddup2(&mut actions, out, 2);
            let spawned = libc::posix_spawn(&mut pid, argv[0].as_ptr(), &actions, std::ptr::null(), args.as_ptr(), env.as_ptr());
            libc::posix_spawn_file_actions_destroy(&mut actions);
            (spawned == 0).then_some(pid)
        }
    }

    /// Wait for `pid`; past `timeout` it is killed. `Some(code)` = it exited
    /// by itself.
    pub fn wait(pid: libc::pid_t, timeout: Duration) -> Option<i32> {
        let started = Instant::now();
        loop {
            let mut status = 0;
            // SAFETY: our own child's pid.
            let reaped = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if reaped == pid {
                return libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status));
            }
            if reaped < 0 {
                return None;
            }
            if started.elapsed() > timeout {
                // SAFETY: as above; reaped right after.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    libc::waitpid(pid, &mut status, 0);
                }
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn cstring(text: &str) -> CString {
        CString::new(text.replace('\0', "")).unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    pub fn exists(path: &CStr) -> bool {
        // SAFETY: a NUL-terminated path.
        unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
    }
}

#[cfg(target_os = "macos")]
impl Sampler for PlatformSampler {
    fn sample(&mut self, pid: u32, path: &Path, out: &mut File) {
        use std::io::Write as _;
        use std::os::fd::AsRawFd as _;
        use std::time::Duration;

        const SAMPLE: &std::ffi::CStr = c"/usr/bin/sample";
        /// Seconds sampled; the symbolication after takes longer.
        const SECONDS: &str = "2";
        const TIMEOUT: Duration = Duration::from_secs(90);

        let _ = writeln!(out, "sampler: /usr/bin/sample {pid} {SECONDS}");
        // `sample` writes its report to a file of its own.
        let report = path.with_extension("sample.tmp");
        let _ = std::fs::remove_file(&report);
        let Ok(null) = File::open("/dev/null") else { return };
        let argv = [SAMPLE.to_owned(), spawn::cstring(&pid.to_string()), spawn::cstring(SECONDS), spawn::cstring("-file"), spawn::cstring(&report.to_string_lossy())];
        match spawn::spawn(&argv, null.as_raw_fd(), out.as_raw_fd()) {
            None => {
                let _ = writeln!(out, "sample did not start");
            }
            Some(child) => match spawn::wait(child, TIMEOUT) {
                Some(0) => {}
                Some(code) => {
                    let _ = writeln!(out, "sample exited with {code}");
                }
                None => {
                    let _ = writeln!(out, "sample did not finish in {} s", TIMEOUT.as_secs());
                }
            },
        }
        if let Ok(report_file) = File::open(&report) {
            let _ = std::io::copy(&mut std::io::Read::take(report_file, crate::MAX_BYTES), out);
        }
        let _ = std::fs::remove_file(&report);
    }
}

#[cfg(target_os = "linux")]
impl Sampler for PlatformSampler {
    fn sample(&mut self, pid: u32, _path: &Path, out: &mut File) {
        use std::io::Write as _;

        task_table(out);
        // The tool stops every thread while it reads, this one included:
        // `timeout` (its parent, not traced) ends a tool that hangs.
        const TIMEOUT: &std::ffi::CStr = c"/usr/bin/timeout";
        const EU_STACK: &std::ffi::CStr = c"/usr/bin/eu-stack";
        const GDB: &std::ffi::CStr = c"/usr/bin/gdb";
        let pid = pid.to_string();
        let (name, seconds, tool): (&str, u64, Vec<&str>) = if spawn::exists(EU_STACK) {
            ("eu-stack", 30, vec!["/usr/bin/eu-stack", "-p", pid.as_str()])
        } else if spawn::exists(GDB) {
            ("gdb", 60, vec!["/usr/bin/gdb", "-p", pid.as_str(), "-batch", "-nx", "-ex", "set pagination off", "-ex", "thread apply all bt 64"])
        } else {
            let _ = writeln!(out, "\nsampler: the task table above is all there is: neither eu-stack nor gdb is installed, so no stacks");
            return;
        };
        if !spawn::exists(TIMEOUT) {
            let _ = writeln!(out, "\nsampler: the task table above is all there is: no /usr/bin/timeout to bound {name}, so no stacks");
            return;
        }
        let _ = writeln!(out, "\nsampler: {name} (at most {seconds} s)");
        match traced(&tool, seconds, out) {
            Some(0) => {}
            // 124 and 137: `timeout`'s own.
            Some(code @ (124 | 137)) => {
                let _ = writeln!(out, "{name} did not finish in {seconds} s ({code})");
            }
            Some(code) => {
                let _ = writeln!(out, "{name} exited with {code}: the task table above may be all there is");
            }
            None => {
                let _ = writeln!(out, "{name} did not run: the task table above is all there is");
            }
        }
    }
}

/// Every thread of this process from `/proc/self/task`: needs nothing.
#[cfg(target_os = "linux")]
fn task_table(out: &mut File) {
    use std::io::Write as _;

    let _ = writeln!(out, "sampler: /proc/self/task (tid, name, state, wait channel, system call)");
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        let _ = writeln!(out, "/proc/self/task is not readable");
        return;
    };
    let mut tids: Vec<u64> = tasks.flatten().filter_map(|task| task.file_name().to_str()?.parse().ok()).collect();
    tids.sort();
    for tid in tids {
        let read = |file: &str| std::fs::read_to_string(format!("/proc/self/task/{tid}/{file}")).map(|text| text.trim().to_string()).unwrap_or_else(|_| "?".into());
        // `pid (comm) S …`: the state follows the last `)`.
        let stat = read("stat");
        let state = stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next()).unwrap_or("?").to_string();
        // The call's number and arguments; `running` = not in one.
        let syscall = read("syscall");
        let _ = writeln!(out, "{tid:>8}  {:<16} {state}  wchan {:<24} syscall {syscall}", read("comm"), read("wchan"));
    }
}

/// Run `tool` (under `timeout`) as this process's tracer: a shell waits on
/// its stdin until this process named it (`PR_SET_PTRACER`), then execs.
#[cfg(target_os = "linux")]
fn traced(tool: &[&str], seconds: u64, out: &mut File) -> Option<i32> {
    use std::os::fd::AsRawFd as _;
    use std::time::Duration;

    let mut argv = vec![
        c"/bin/sh".to_owned(),
        c"-c".to_owned(),
        c"read _ && exec /usr/bin/timeout -s KILL \"$@\"".to_owned(),
        c"sh".to_owned(),
        spawn::cstring(&seconds.to_string()),
    ];
    argv.extend(tool.iter().map(|arg| spawn::cstring(arg)));
    let mut pipe = [0; 2];
    // SAFETY: two fds out; both closed below.
    if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return None;
    }
    let child = spawn::spawn(&argv, pipe[0], out.as_raw_fd());
    // SAFETY: our own pipe; prctl with plain integers.
    unsafe {
        libc::close(pipe[0]);
        if let Some(child) = child {
            libc::prctl(libc::PR_SET_PTRACER, child as libc::c_ulong, 0, 0, 0);
            libc::write(pipe[1], c"\n".as_ptr().cast(), 1);
        }
        libc::close(pipe[1]);
    }
    // `timeout` kills the tool; this bound is for `timeout` itself.
    let code = child.and_then(|child| spawn::wait(child, Duration::from_secs(seconds + 10)));
    // SAFETY: as above; 0 = no tracer named.
    unsafe {
        libc::prctl(libc::PR_SET_PTRACER, 0, 0, 0, 0);
    }
    code
}
