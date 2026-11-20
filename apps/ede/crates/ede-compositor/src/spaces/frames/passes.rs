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
    reexports::calloop::{
        channel::{self, Event as ChannelEvent},
        generic::Generic,
        timer::{TimeoutAction, Timer},
        Interest, Mode, PostAction,
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
        self.spaces.frames.gpu = match context.and_then(|c| SpacesGpu::new(c, picked.node.clone())) {
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

    /// Tests: spaces render on Mesa's software EGL device (which `auto`
    /// never takes), so the GPU path runs without a GPU.
    pub fn open_space_gpu_for_tests(&mut self) -> Result<(), String> {
        self.spaces.frames.gpu = Gpu::Ready(Box::new(SpacesGpu::software_for_tests()?));
        Ok(())
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
        let ready = matches!(self.spaces.frames.gpu, Gpu::Ready(_));
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
        self.space_used(desk);
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
