//! #EDE-162: a space's capture on the GPU ([`super::gpu`]), from the queue
//! ([`super::queue`]): opened once off the person's thread; a pass is
//! submitted after the person's frames and answered when its fence fires,
//! so the person's thread never waits for the GPU. A space drawn on the GPU
//! once stays on it for its life (its GPU clients' buffers import nowhere
//! else); a pass that cannot be submitted is drawn in software instead.

use std::time::{Duration, Instant};

use ede_protocol::{CaptureInfo, ErrorCode, Failure};
use smithay::{
    backend::{
        egl::EGLContext,
        renderer::gles::GlesRenderer,
        SwapBuffersError,
    },
    reexports::{
        calloop::{
            channel::{self, Event as ChannelEvent},
            generic::Generic,
            timer::{TimeoutAction, Timer},
            Interest, Mode, PostAction,
        },
    },
};

use super::{
    gpu::{self, InFlight, Picked, SpacesGpu},
    queue::Waiting,
    stats::thread_cpu,
};
use crate::{backend::OutputElements, desk::DeskId, state::Ede};

/// Where the spaces' GPU renderer is.
#[derive(Default)]
pub(crate) enum Gpu {
    /// Not opened: spaces draw in software (and why).
    #[default]
    Off,
    Software(String),
    Opening,
    Ready(Box<SpacesGpu>),
}

/// One pass on its way back, and who waits for it.
pub(crate) struct Pass {
    flight: InFlight<OutputElements<GlesRenderer>>,
    jobs: Vec<Waiting>,
    submitted: Instant,
    /// The space's serial its pixels show.
    serial: u64,
}

/// GPU buffers one client of a space may make per second (each import is
/// work on the person's thread; a swapchain makes three or four).
pub const IMPORTS_PER_SECOND: usize = 32;

/// How long a pass may take before its fence is given up on.
const FENCE_TIMEOUT: Duration = Duration::from_secs(2);
/// The poll of a fence that has no fd (no EGL_ANDROID_native_fence_sync).
const FENCE_POLL: Duration = Duration::from_millis(1);

impl Ede {
    /// Opens the spaces' GPU renderer as `[space_gpu] device` says, off the
    /// person's thread. Backends call it once; `safe_mode` keeps spaces in
    /// software.
    pub fn start_space_gpu(&mut self, safe_mode: bool) {
        let why = |reason: String| {
            tracing::info!("agent spaces render in software: {reason}");
            Gpu::Software(reason)
        };
        if safe_mode {
            self.spaces.frames.gpu = why("safe mode".into());
            return;
        }
        let picked = match gpu::Wanted::parse(&self.config.space_gpu.device).and_then(|wanted| gpu::this_machine_choice(&wanted)) {
            Ok(picked) => picked,
            Err(reason) => {
                self.spaces.frames.gpu = why(reason);
                return;
            }
        };
        let (tx, rx) = channel::channel::<Result<EGLContext, String>>();
        let node = picked.node.clone();
        let spawned = std::thread::Builder::new().name("ede-space-gpu".into()).spawn(move || {
            let _ = tx.send(gpu::open_context(&node));
        });
        if let Err(e) = spawned {
            self.spaces.frames.gpu = why(format!("no thread to open the GPU on: {e}"));
            return;
        }
        let inserted = self.loop_handle.insert_source(rx, move |event, _, state| match event {
            ChannelEvent::Msg(context) => state.space_gpu_opened(&picked, context),
            ChannelEvent::Closed => {
                if matches!(state.spaces.frames.gpu, Gpu::Opening) {
                    state.spaces.frames.gpu = Gpu::Software("the GPU thread ended".into());
                }
            }
        });
        self.spaces.frames.gpu = match inserted {
            Ok(_) => Gpu::Opening,
            Err(e) => why(format!("{e}")),
        };
    }

    fn space_gpu_opened(&mut self, picked: &Picked, context: Result<EGLContext, String>) {
        self.spaces.frames.gpu = match context.and_then(|c| SpacesGpu::new(c, picked.node.clone())).map(|mut gpu| {
            gpu.driver = picked.driver.clone();
            gpu
        }).and_then(|gpu| self.space_gpu_buffers(picked, gpu)) {
            Ok(gpu) => {
                tracing::info!(
                    "agent spaces render on {} ({} {}, {}; priority {:?})",
                    picked.node.display(),
                    picked.card.as_deref().unwrap_or("?"),
                    picked.driver.as_deref().unwrap_or("?"),
                    gpu.name,
                    gpu.priority
                );
                Gpu::Ready(Box::new(gpu))
            }
            Err(reason) => {
                tracing::warn!("agent spaces render in software: {reason}");
                Gpu::Software(reason)
            }
        };
    }

    /// Offers a GPU space's clients linux-dmabuf on the spaces' renderer
    /// (its device as the main device, the formats and explicit modifiers it
    /// imports: an implicit modifier across devices could import and draw
    /// garbage) and drm-syncobj on its render node when no backend made one.
    /// An NVIDIA renderer without explicit sync offers none: its buffers carry
    /// no implicit fence, a capture would read frames half drawn.
    fn space_gpu_buffers(&mut self, picked: &Picked, gpu: SpacesGpu) -> Result<SpacesGpu, String> {
        use smithay::{
            backend::{
                allocator::{format::FormatSet, Modifier},
                drm::{DrmDeviceFd, DrmNode},
                renderer::ImportDma,
            },
            utils::DeviceFd,
            wayland::{
                dmabuf::DmabufFeedbackBuilder,
                drm_syncobj::{supports_syncobj_eventfd, DrmSyncobjState},
            },
        };
        let formats: FormatSet = gpu.renderer.dmabuf_formats().iter().filter(|f| f.modifier != Modifier::Invalid).copied().collect();
        if formats.iter().next().is_none() {
            return Err(format!("{}: no explicit-modifier format to offer", gpu.name));
        }
        if self.syncobj_state.is_none() {
            let opened = std::fs::OpenOptions::new().read(true).write(true).open(&picked.node).map_err(|e| e.to_string());
            match opened.map(|f| DrmDeviceFd::new(DeviceFd::from(std::os::fd::OwnedFd::from(f)))) {
                Ok(fd) if supports_syncobj_eventfd(&fd) => {
                    self.syncobj_state = Some(DrmSyncobjState::new_with_filter::<Ede, _>(&self.display_handle, fd, crate::desk::on_gpu_space));
                }
                _ => tracing::info!("agent spaces: no explicit sync (drm-syncobj) on {}", picked.node.display()),
            }
        }
        if picked.driver.as_deref() == Some("nvidia") && self.syncobj_state.is_none() {
            return Err(format!("{}: NVIDIA without explicit sync (drm-syncobj) on its render node", picked.node.display()));
        }
        let node = DrmNode::from_path(&picked.node).map_err(|e| e.to_string())?;
        let feedback = DmabufFeedbackBuilder::new(node.dev_id(), formats).build().map_err(|e| e.to_string())?;
        let global = self.dmabuf_state.create_global_with_filter_and_default_feedback::<Ede, _>(&self.display_handle, &feedback, crate::desk::on_gpu_space);
        self.spaces.frames.dmabuf = Some(global);
        Ok(gpu)
    }

    /// Tests: spaces render on Mesa's software EGL device (which `auto`
    /// never takes), so the GPU path runs without a GPU, and its clients
    /// are offered linux-dmabuf (without feedback: the device has no node).
    pub fn open_space_gpu_for_tests(&mut self) -> Result<(), String> {
        use smithay::backend::{
            allocator::{format::FormatSet, Format, Fourcc, Modifier},
            renderer::ImportDma,
        };
        let gpu = SpacesGpu::software_for_tests()?;
        let mut formats: FormatSet = gpu.renderer.dmabuf_formats().iter().filter(|f| f.modifier != Modifier::Invalid).copied().collect();
        if formats.iter().next().is_none() {
            formats = [Format { code: Fourcc::Argb8888, modifier: Modifier::Linear }].into_iter().collect();
        }
        let global = self.dmabuf_state.create_global_with_filter::<Ede, _>(&self.display_handle, formats, crate::desk::on_gpu_space);
        self.spaces.frames.dmabuf = Some(global);
        self.spaces.frames.gpu = Gpu::Ready(Box::new(gpu));
        Ok(())
    }

    /// The kernel driver of the GPU spaces render on, when they do.
    pub(crate) fn space_gpu_driver(&self) -> Option<&str> {
        match &self.spaces.frames.gpu {
            Gpu::Ready(gpu) => gpu.driver.as_deref(),
            _ => None,
        }
    }

    /// A GPU space's clients are offered linux-dmabuf.
    pub(super) fn space_offers_gpu_buffers(&self) -> bool {
        self.spaces.frames.dmabuf.is_some() && matches!(self.spaces.frames.gpu, Gpu::Ready(_))
    }

    pub(crate) fn is_spaces_dmabuf_global(&self, global: &smithay::wayland::dmabuf::DmabufGlobal) -> bool {
        self.spaces.frames.dmabuf.as_ref() == Some(global)
    }

    /// A GPU space's client made a dmabuf: it is imported (so validated)
    /// on the spaces' renderer, the one that will draw it, within a budget
    /// of [`IMPORTS_PER_SECOND`] per client (each import is work on the
    /// person's thread). A refused one latches the client's space.
    pub(crate) fn space_dmabuf_imported(&mut self, dmabuf: smithay::backend::allocator::dmabuf::Dmabuf, notifier: smithay::wayland::dmabuf::ImportNotifier) {
        use smithay::backend::{allocator::Buffer as _, renderer::ImportDma};
        let client = notifier.client();
        let desk = client.as_ref().and_then(crate::desk::desk_of_client);
        // Clients with no import in the last second are forgotten (gone ones too).
        self.spaces.frames.imports.retain(|_, recent| recent.back().is_some_and(|at| at.elapsed() < Duration::from_secs(1)));
        let over = client.as_ref().is_some_and(|c| {
            let recent = self.spaces.frames.imports.entry(c.id()).or_default();
            recent.retain(|at| at.elapsed() < Duration::from_secs(1));
            recent.push_back(Instant::now());
            recent.len() > IMPORTS_PER_SECOND
        });
        let started = Instant::now();
        let imported = !over
            && match &mut self.spaces.frames.gpu {
                Gpu::Ready(gpu) => gpu.renderer.import_dmabuf(&dmabuf, None).is_ok(),
                _ => false,
            };
        let took = started.elapsed();
        if took > Duration::from_millis(5) {
            tracing::info!("a space's GPU buffer took {took:?} to import ({:?} {}x{})", dmabuf.format(), dmabuf.width(), dmabuf.height());
        }
        if imported {
            let _ = notifier.successful::<Ede>();
            return;
        }
        notifier.failed();
        if let Some(desk) = desk {
            let why = if over { format!("an app made more than {IMPORTS_PER_SECOND} GPU buffers a second") } else { format!("an app's GPU buffer ({:?}) cannot be drawn on the spaces' GPU", dmabuf.format()) };
            self.space_latch(desk, why);
        }
    }

    /// What spaces render on: `Ok(GL_RENDERER)` or why they are in software.
    pub fn space_rendering(&self) -> Result<String, String> {
        match &self.spaces.frames.gpu {
            Gpu::Ready(gpu) => Ok(gpu.name.clone()),
            Gpu::Off => Err("not opened".into()),
            Gpu::Opening => Err("opening".into()),
            Gpu::Software(why) => Err(why.clone()),
        }
    }

    /// Space `desk` is captured on the GPU: decided at its first capture
    /// and kept for its life.
    pub fn space_on_gpu(&mut self, desk: DeskId) -> bool {
        let ready = matches!(self.spaces.frames.gpu, Gpu::Ready(_)) && self.spaces_may_start_on_gpu();
        *self.spaces.frames.on_gpu.entry(desk).or_insert(ready)
    }

    /// A pass of space `desk` is on the GPU now.
    pub(super) fn space_gpu_busy(&self, desk: DeskId) -> bool {
        self.spaces.frames.passes.values().any(|p| p.flight.desk == desk)
    }

    /// Submits a pass of `desk` for `jobs`; `Err(jobs)` = draw them in
    /// software now. The thread time it took goes into the stats.
    pub(super) fn space_gpu_capture(&mut self, desk: DeskId, jobs: Vec<Waiting>) -> Result<Duration, Vec<Waiting>> {
        let Some(size) = self.spaces.get(desk).and_then(|s| s.output.current_mode()).map(|m| (m.size.w, m.size.h)) else { return Err(jobs) };
        let clear = self.config.background_rgba().unwrap_or([0.04, 0.04, 0.04, 1.0]);
        let cpu = thread_cpu();
        let mut gpu = match std::mem::take(&mut self.spaces.frames.gpu) {
            Gpu::Ready(gpu) => gpu,
            other => {
                self.spaces.frames.gpu = other;
                return Err(jobs);
            }
        };
        // #EXP-2024: the space's own captures are use; a watcher's are not.
        if jobs.iter().any(|j| j.own) {
            self.space_used(desk);
        }
        let (elements, undrawable) = self.space_elements(desk, &mut gpu.renderer);
        let serial = self.spaces.get(desk).map_or(0, |s| s.serial);
        let submitted = gpu.submit(desk, size, clear, elements);
        let live: Vec<DeskId> = self.spaces.iter().map(|s| s.desk.id).collect();
        gpu.sweep(&live);
        self.spaces.frames.gpu = Gpu::Ready(gpu);
        if let Some(app) = undrawable {
            self.space_undrawable(desk, &app);
        }
        let flight = match submitted {
            Ok(flight) => flight,
            Err(e) => {
                self.space_gpu_failed(&format!("a capture of {desk:?}: {e}"), matches!(SwapBuffersError::from(e), SwapBuffersError::ContextLost(_)));
                return Err(jobs);
            }
        };
        let id = self.spaces.frames.next_pass;
        self.spaces.frames.next_pass += 1;
        let waiting = match flight.fence.export() {
            Ok(fd) => self
                .loop_handle
                .insert_source(Generic::new(fd, Interest::READ, Mode::OneShot), move |_, _, state| {
                    state.space_gpu_done(id);
                    Ok(PostAction::Remove)
                })
                .map(|_| ())
                .map_err(|e| e.to_string()),
            // No fence fd: look again every millisecond.
            Err(_) => self
                .loop_handle
                .insert_source(Timer::from_duration(FENCE_POLL), move |_, _, state| match state.space_gpu_signalled(id) {
                    true => {
                        state.space_gpu_done(id);
                        TimeoutAction::Drop
                    }
                    false => TimeoutAction::ToDuration(FENCE_POLL),
                })
                .map(|_| ())
                .map_err(|e| e.to_string()),
        };
        // A GPU that never answers does not hold the space's captures: the
        // pass is given up, without reading it (that would wait).
        let deadline = self.loop_handle.insert_source(Timer::from_duration(FENCE_TIMEOUT), move |_, _, state| {
            state.space_gpu_given_up(id);
            TimeoutAction::Drop
        });
        if let Err(e) = waiting.and(deadline.map(|_| ()).map_err(|e| e.to_string())) {
            tracing::warn!("a space capture's fence cannot be waited on: {e}");
            return Err(jobs);
        }
        self.spaces.frames.passes.insert(id, Pass { flight, jobs, submitted: Instant::now(), serial });
        self.space_gpu_sweep_later();
        Ok(thread_cpu().saturating_sub(cpu))
    }

    /// The pass's fence fired (or the pass is gone: nothing to poll for).
    fn space_gpu_signalled(&mut self, id: u64) -> bool {
        let Some(pass) = self.spaces.frames.passes.get(&id) else { return true };
        pass.flight.fence.client_wait(Some(Duration::ZERO), false).unwrap_or(true)
    }

    /// Pass `id` took [`FENCE_TIMEOUT`]: its callers get an error, every
    /// target draws whole next time.
    fn space_gpu_given_up(&mut self, id: u64) {
        let Some(Pass { jobs, .. }) = self.spaces.frames.passes.remove(&id) else { return };
        self.space_gpu_failed(&format!("a capture's fence did not fire within {FENCE_TIMEOUT:?}"), false);
        for job in jobs {
            self.ipc_reply_later(job.caller.conn, job.caller.call, Err(Failure::new(ErrorCode::Internal, "the GPU did not answer")));
        }
    }

    /// The GPU finished pass `id`: its pixels go to everyone who waited.
    fn space_gpu_done(&mut self, id: u64) {
        let Some(Pass { flight, jobs, submitted, serial }) = self.spaces.frames.passes.remove(&id) else { return };
        let cpu = thread_cpu();
        let pixels = match &mut self.spaces.frames.gpu {
            Gpu::Ready(gpu) => gpu.read(&flight).map_err(|e| e.to_string()),
            _ => Err("the GPU renderer is gone".to_string()),
        };
        let (w, h, desk) = (flight.size.0, flight.size.1, flight.desk);
        // The clients' buffers go back now that the GPU read them.
        drop(flight);
        self.spaces.frames.stats.gpu_done(thread_cpu().saturating_sub(cpu), submitted.elapsed());
        let serial = self.spaces.get(desk).map(|_| serial);
        match (pixels, serial) {
            (Ok(bytes), Some(serial)) => {
                let info = serde_json::to_value(CaptureInfo { width: w, height: h, stride: w * 4, format: "argb8888".into(), serial, unchanged: false, notice: None }).unwrap();
                let notice = jobs.iter().any(|j| j.own).then(|| self.space_take_notice(desk)).flatten();
                for job in jobs {
                    let info = super::queue::with_notice(info.clone(), notice.as_ref().filter(|_| job.own));
                    self.ipc_reply_later_with(job.caller.conn, job.caller.call, Ok((info, Some(bytes.clone()))));
                }
            }
            (_, None) => {
                for job in jobs {
                    self.ipc_reply_later(job.caller.conn, job.caller.call, Err(Failure::new(ErrorCode::NotFound, "no such space")));
                }
            }
            (Err(e), Some(_)) => {
                self.space_gpu_failed(&format!("reading a capture back: {e}"), false);
                for job in jobs {
                    self.ipc_reply_later(job.caller.conn, job.caller.call, Err(Failure::new(ErrorCode::Internal, &e)));
                }
            }
        }
    }

    /// A GPU pass failed. A lost context ends the GPU path for this run
    /// (spaces draw in software); otherwise every target draws whole next
    /// time.
    fn space_gpu_failed(&mut self, what: &str, lost: bool) {
        tracing::warn!("agent spaces on the GPU: {what}");
        if lost {
            self.spaces.frames.gpu = Gpu::Software(format!("the GPU context was lost ({what})"));
            let on_gpu: Vec<DeskId> = self.spaces.frames.on_gpu.iter().filter(|(_, on)| **on).map(|(desk, _)| *desk).collect();
            for desk in on_gpu {
                self.space_latch(desk, "the GPU renderer's context was lost".into());
            }
            self.spaces.frames.on_gpu.clear();
        } else if let Gpu::Ready(gpu) = &mut self.spaces.frames.gpu {
            gpu.reset_targets();
        }
    }

    /// Every [`gpu::TARGET_IDLE`] while targets exist: the idle ones give
    /// their GPU memory back.
    fn space_gpu_sweep_later(&mut self) {
        if self.spaces.frames.sweeping {
            return;
        }
        let armed = self.loop_handle.insert_source(Timer::from_duration(gpu::TARGET_IDLE), |_, _, state| {
            let live: Vec<DeskId> = state.spaces.iter().map(|s| s.desk.id).collect();
            let again = match &mut state.spaces.frames.gpu {
                Gpu::Ready(gpu) => {
                    gpu.sweep(&live);
                    gpu.has_targets()
                }
                _ => false,
            };
            state.spaces.frames.on_gpu.retain(|desk, _| live.contains(desk));
            state.spaces.frames.sweeping = again;
            if again {
                TimeoutAction::ToDuration(gpu::TARGET_IDLE)
            } else {
                TimeoutAction::Drop
            }
        });
        self.spaces.frames.sweeping = armed.is_ok();
    }
}
