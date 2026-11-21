//! linux-dmabuf: GPU clients (GL/Vulkan apps, Xwayland's glamor, games)
//! hand us GPU buffers instead of copying through shm. The backend that owns
//! the renderer installs the importer and creates the global; until then
//! (headless) clients fall back to shm. With the udev backend, drm-syncobj
//! (explicit sync, which NVIDIA needs) makes a commit wait for its acquire
//! point before it applies.

use smithay::{
    backend::allocator::dmabuf::Dmabuf,
    reexports::{calloop::Interest, wayland_server::Resource},
    wayland::{
        compositor::{add_blocker, add_pre_commit_hook, with_states, BufferAssignment, CompositorHandler, SurfaceAttributes},
        dmabuf::{get_dmabuf, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
    },
};

use crate::state::Ede;

impl DmabufHandler for Ede {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(&mut self, global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        // #EDE-162: the spaces' own global imports on the spaces' renderer.
        #[cfg(feature = "gpu")]
        if self.is_spaces_dmabuf_global(global) {
            return self.space_dmabuf_imported(dmabuf, notifier);
        }
        #[cfg(not(feature = "gpu"))]
        let _ = global;
        let ok = self.dmabuf_importer.as_mut().is_some_and(|import| import(&dmabuf));
        if ok {
            let _ = notifier.successful::<Ede>();
        } else {
            notifier.failed();
        }
    }
}

#[cfg(feature = "gpu")]
impl smithay::wayland::drm_syncobj::DrmSyncobjHandler for Ede {
    fn drm_syncobj_state(&mut self) -> Option<&mut smithay::wayland::drm_syncobj::DrmSyncobjState> {
        self.syncobj_state.as_mut()
    }
}

/// #EDE-162: how long a space's GPU buffer may wait for its acquire fence.
#[cfg(feature = "gpu")]
const ACQUIRE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// A commit carrying a dmabuf waits until the GPU finished writing it: the
/// syncobj acquire point if the client set one, else the dmabuf's implicit
/// fence. Installed on every new surface.
pub fn install_blockers(surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) {
    add_pre_commit_hook::<Ede, _>(surface, |state, _dh, surface| {
        #[cfg(feature = "gpu")]
        let mut acquire = None;
        let dmabuf = with_states(surface, |data| {
            #[cfg(feature = "gpu")]
            acquire.clone_from(&data.cached_state.get::<smithay::wayland::drm_syncobj::DrmSyncobjCachedState>().pending().acquire_point);
            data.cached_state.get::<SurfaceAttributes>().pending().buffer.as_ref().and_then(|b| match b {
                BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                _ => None,
            })
        });
        let Some(dmabuf) = dmabuf else { return };
        let Some(client) = surface.client() else { return };
        #[cfg(feature = "gpu")]
        if let Some(point) = acquire {
            if let Ok((blocker, source)) = point.generate_blocker() {
                let c = client.clone();
                let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let set = fired.clone();
                let inserted = state.loop_handle.insert_source(source, move |_, _, state| {
                    set.store(true, std::sync::atomic::Ordering::Relaxed);
                    let dh = state.display_handle.clone();
                    state.client_compositor_state(&c).blocker_cleared(state, &dh);
                    Ok(())
                });
                if inserted.is_ok() {
                    add_blocker(surface, blocker);
                    // #EDE-162: a space's GPU buffer whose fence never fires
                    // would freeze its window: the space latches.
                    if let Some(desk) = crate::spaces::space_of_surface(surface) {
                        let _ = state.loop_handle.insert_source(smithay::reexports::calloop::timer::Timer::from_duration(ACQUIRE_TIMEOUT), move |_, _, state| {
                            if !fired.load(std::sync::atomic::Ordering::Relaxed) {
                                state.space_latch(desk, "an app's GPU buffer never became ready (its fence did not fire within 2 s)".into());
                            }
                            smithay::reexports::calloop::timer::TimeoutAction::Drop
                        });
                    }
                    return;
                }
            }
        }
        if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
            let inserted = state.loop_handle.insert_source(source, move |_, _, state| {
                let dh = state.display_handle.clone();
                state.client_compositor_state(&client).blocker_cleared(state, &dh);
                Ok(())
            });
            if inserted.is_ok() {
                add_blocker(surface, blocker);
            }
        }
    });
}
