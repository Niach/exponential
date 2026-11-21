//! #EDE-162: the safety net. An app whose GPU path fails in a space falls
//! back to software, never a blank window. In a space that renders on the
//! GPU, the compositor watches for failures it can see itself:
//!
//! - a launch that ends non-zero or by a signal within [`START`] while no
//!   client of it (Wayland, linked by the launch's process group) ever
//!   committed a buffer: vkcube's "no present queue", ANV's swapchain
//!   abort, a Wine or GLFW X11 Vulkan app (an X11 app's buffers come
//!   through the space's Xwayland, which is no client of the launch, so it
//!   always counts as "drew nothing": a false alarm costs GPU speed only).
//!   127 is not counted: it is a launch's own stage failing, not the app.
//!   A process of the launch that is writing a core dump counts at once: a
//!   crashed app can take half a minute to end while its core is written;
//! - a buffer of the space the GPU renderer cannot draw (its window is drawn
//!   as a grey placeholder, never left blank or closed);
//! - the GPU renderer's context lost.
//!
//! Then the space latches back to software: its later clients and launches
//! draw in software (F3's switch, the spaces' dmabuf hidden from new
//! clients), its running GPU clients stay captured on the GPU, and the
//! agent reads it with its next capture: "X ended at start on the GPU
//! (...); this space now draws in software: start it again". After
//! [`LATCHES_MAX`] latched spaces in one run, new spaces start in software.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use std::os::unix::process::ExitStatusExt;

use smithay::reexports::{
    calloop::timer::{TimeoutAction, Timer},
    rustix::process::{getpgid, Pid},
    wayland_server::{backend::ClientId, Client},
};

use crate::{desk::DeskId, state::Ede};

/// A launch that fails within this long counts as a GPU path that failed.
pub const START: Duration = Duration::from_secs(10);
/// How often a watched launch's processes are looked at for a core dump
/// in progress: a crashed app can take half a minute to end while its core
/// is written (systemd-coredump), long after it failed.
const CRASH_POLL: Duration = Duration::from_millis(250);
/// After this many latched spaces in one run, new spaces draw in software.
pub const LATCHES_MAX: usize = 3;
/// What an undrawable window shows instead of being blank.
pub const PLACEHOLDER: [f32; 4] = [0.19, 0.19, 0.19, 1.0];

#[derive(Default)]
pub(crate) struct Net {
    watched: Vec<Watched>,
    /// Per space, the process groups of clients that committed a buffer.
    drew: HashMap<DeskId, HashSet<i32>>,
    /// A client's process group, asked once.
    groups: HashMap<ClientId, Option<i32>>,
    /// Spaces that latched back to software, and why.
    latched: HashMap<DeskId, String>,
    /// What the space's agent reads with its next capture.
    notices: HashMap<DeskId, String>,
    /// Test only: spaces treated as rendering on the GPU without a renderer,
    /// and the kernel driver of that GPU.
    pretend: HashSet<DeskId>,
    pretend_driver: Option<String>,
    /// The crash poll is armed.
    polling: bool,
}

struct Watched {
    desk: DeskId,
    pid: u32,
    app: String,
    started: Instant,
}

/// Whether `pid` or a process under it is writing a core dump
/// (`CoreDumping: 1` in its status, Linux 4.15 on).
fn dumping_core(pid: u32) -> bool {
    let mut todo = vec![pid];
    let mut seen = 0;
    while let Some(p) = todo.pop() {
        seen += 1;
        if seen > 256 {
            break;
        }
        if std::fs::read_to_string(format!("/proc/{p}/status")).is_ok_and(|s| s.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == ["CoreDumping:", "1"])) {
            return true;
        }
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{p}/task")) else { continue };
        for task in tasks.flatten() {
            let children = std::fs::read_to_string(task.path().join("children")).unwrap_or_default();
            todo.extend(children.split_whitespace().filter_map(|c| c.parse::<u32>().ok()));
        }
    }
    false
}

/// A launch's end that counts as a failed GPU path.
pub fn failed_start(code: Option<i32>, signal: Option<i32>) -> Option<String> {
    match (code, signal) {
        (_, Some(signal)) => Some(format!("signal {signal}")),
        (Some(0), _) | (Some(127), _) => None,
        (Some(code), _) => Some(format!("exit {code}")),
        (None, None) => None,
    }
}

impl Ede {
    /// Space `desk` draws on the GPU and has not latched back to software.
    pub fn space_gpu_allowed(&mut self, desk: DeskId) -> bool {
        (self.spaces.frames.net.pretend.contains(&desk) || (self.space_offers_gpu_buffers() && self.space_on_gpu(desk))) && !self.spaces.frames.net.latched.contains_key(&desk)
    }

    /// Why space `desk` latched back to software, if it did.
    pub fn space_latched(&self, desk: DeskId) -> Option<&str> {
        self.spaces.frames.net.latched.get(&desk).map(String::as_str)
    }

    /// New spaces may still start on the GPU ([`LATCHES_MAX`]).
    pub fn spaces_may_start_on_gpu(&self) -> bool {
        self.spaces.frames.net.latched.len() < LATCHES_MAX
    }

    /// Tests: space `desk` counts as drawing on the GPU, so the net watches
    /// it, without a GPU renderer.
    pub fn pretend_space_on_gpu(&mut self, desk: DeskId) {
        self.spaces.frames.net.pretend.insert(desk);
    }

    /// Tests: the GPU of [`Self::pretend_space_on_gpu`] has kernel driver
    /// `driver` (what a space's Vulkan is pointed at follows it).
    pub fn pretend_space_gpu_driver(&mut self, driver: &str) {
        self.spaces.frames.net.pretend_driver = Some(driver.into());
    }

    pub(crate) fn space_pretended_driver(&self) -> Option<&str> {
        self.spaces.frames.net.pretend_driver.as_deref()
    }

    /// A launch of space `desk` started: watched for [`START`] if the space
    /// draws on the GPU.
    pub(crate) fn space_net_launched(&mut self, desk: DeskId, pid: u32, app: &str) {
        self.space_net_sweep();
        if !self.space_gpu_allowed(desk) {
            return;
        }
        let app = std::path::Path::new(app).file_name().map_or_else(|| app.to_string(), |n| n.to_string_lossy().into_owned());
        self.spaces.frames.net.watched.push(Watched { desk, pid, app, started: Instant::now() });
        if !self.spaces.frames.net.polling {
            let armed = self.loop_handle.insert_source(Timer::from_duration(CRASH_POLL), |_, _, state| match state.space_net_crashes() {
                true => TimeoutAction::ToDuration(CRASH_POLL),
                false => TimeoutAction::Drop,
            });
            self.spaces.frames.net.polling = armed.is_ok();
        }
    }

    /// A watched launch whose process tree dumps a core crashed: counted
    /// at once (its end comes when the dump is written). `false` = nothing
    /// left to watch.
    fn space_net_crashes(&mut self) -> bool {
        let net = &mut self.spaces.frames.net;
        net.watched.retain(|w| w.started.elapsed() <= START);
        let crashed: Vec<usize> = net.watched.iter().enumerate().filter(|(_, w)| dumping_core(w.pid)).map(|(i, _)| i).collect();
        for i in crashed.into_iter().rev() {
            let w = self.spaces.frames.net.watched.remove(i);
            let drew = self.spaces.frames.net.drew.get(&w.desk).is_some_and(|groups| groups.contains(&(w.pid as i32)));
            if !drew {
                self.space_latch(w.desk, format!("{} crashed at start on the GPU", w.app));
            }
        }
        let net = &mut self.spaces.frames.net;
        net.polling = !net.watched.is_empty();
        net.polling
    }

    /// A client of space `desk` committed a new buffer: its launch drew.
    pub(crate) fn space_net_drew(&mut self, desk: DeskId, client: Option<Client>) {
        let Some(client) = client else { return };
        let dh = self.display_handle.clone();
        let net = &mut self.spaces.frames.net;
        let group = *net.groups.entry(client.id()).or_insert_with(|| {
            let pid = client.get_credentials(&dh).ok()?.pid;
            getpgid(Pid::from_raw(pid)).ok().map(|g| g.as_raw_nonzero().get())
        });
        if let Some(group) = group {
            net.drew.entry(desk).or_default().insert(group);
        }
    }

    /// A launch ended (the launches' tick reaped it): a watched one that
    /// failed within [`START`] and never drew latches its space.
    pub(crate) fn space_net_ended(&mut self, pid: u32, status: std::process::ExitStatus) {
        let net = &mut self.spaces.frames.net;
        net.watched.retain(|w| w.started.elapsed() <= START);
        let Some(at) = net.watched.iter().position(|w| w.pid == pid) else { return };
        let w = net.watched.remove(at);
        let drew = net.drew.get(&w.desk).is_some_and(|groups| groups.contains(&(w.pid as i32)));
        if let Some(how) = failed_start(status.code(), status.signal()).filter(|_| !drew) {
            self.space_latch(w.desk, format!("{} ended at start on the GPU ({how})", w.app));
        }
    }

    /// Space `desk` draws in software from now on (its running GPU clients
    /// stay captured on the GPU); the agent reads why at its next capture.
    pub(crate) fn space_latch(&mut self, desk: DeskId, why: String) {
        if self.spaces.get(desk).is_none() || self.spaces.frames.net.latched.contains_key(&desk) {
            return;
        }
        tracing::warn!("{desk:?}: {why}; the space draws in software from now on");
        let notice = format!("{why}; this space now draws in software: start it again.");
        self.spaces.frames.net.notices.insert(desk, notice);
        // Its later launches see lavapipe alone (F3's switch, read at each launch).
        self.set_space_software_rendering(desk, true);
        self.spaces.frames.net.latched.insert(desk, why);
        if !self.spaces_may_start_on_gpu() {
            tracing::warn!("{LATCHES_MAX} spaces fell back to software in this run: new spaces start in software");
        }
    }

    /// `app`'s buffers could not be drawn in space `desk` (a grey
    /// placeholder shows instead): on a GPU space, it latches.
    pub(crate) fn space_undrawable(&mut self, desk: DeskId, app: &str) {
        if self.space_gpu_allowed(desk) {
            self.space_latch(desk, format!("{app} could not be drawn on the GPU"));
        }
    }

    /// What space `desk`'s agent should read, once.
    pub(crate) fn space_take_notice(&mut self, desk: DeskId) -> Option<String> {
        self.spaces.frames.net.notices.remove(&desk)
    }

    /// Forgets spaces that closed.
    pub(crate) fn space_net_sweep(&mut self) {
        let live: Vec<DeskId> = self.spaces.iter().map(|s| s.desk.id).collect();
        let net = &mut self.spaces.frames.net;
        net.drew.retain(|desk, _| live.contains(desk));
        net.notices.retain(|desk, _| live.contains(desk));
        net.pretend.retain(|desk| live.contains(desk));
        let alive = |id: &ClientId| self.display_handle.backend_handle().get_client_data(id.clone()).is_ok();
        net.groups.retain(|id, _| alive(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_process_is_not_dumping_a_core() {
        assert!(!dumping_core(std::process::id()));
        assert!(!dumping_core(u32::MAX), "a process that is gone");
    }

    #[test]
    fn a_start_fails_on_a_signal_or_a_non_zero_exit_but_not_on_127_or_0() {
        assert_eq!(failed_start(Some(1), None).as_deref(), Some("exit 1"));
        assert_eq!(failed_start(Some(134), None).as_deref(), Some("exit 134"));
        assert_eq!(failed_start(None, Some(6)).as_deref(), Some("signal 6"));
        assert_eq!(failed_start(Some(0), None), None);
        assert_eq!(failed_start(Some(127), None), None, "a launch's own stage failed: not the app's GPU path");
        assert_eq!(failed_start(None, None), None);
    }
}
