//! linux-dmabuf: GPU clients (GL/Vulkan apps, Xwayland's glamor, games)
//! hand us GPU buffers instead of copying through shm. The backend that owns
//! the renderer installs the importer and creates the global; until then
//! (headless) clients fall back to shm. With the udev backend, drm-syncobj
//! (explicit sync, which NVIDIA needs) makes a commit wait for its acquire
//! point before it applies.

use smithay::{
    backend::allocator::dmabuf::Dmabuf,
    delegate_dmabuf,
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

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let ok = self.dmabuf_importer.as_mut().is_some_and(|import| import(&dmabuf));
        if ok {
            let _ = notifier.successful::<Ede>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(Ede);

#[cfg(feature = "udev")]
impl smithay::wayland::drm_syncobj::DrmSyncobjHandler for Ede {
    fn drm_syncobj_state(&mut self) -> Option<&mut smithay::wayland::drm_syncobj::DrmSyncobjState> {
        self.syncobj_state.as_mut()
    }
}
#[cfg(feature = "udev")]
smithay::delegate_drm_syncobj!(Ede);

/// A commit carrying a dmabuf waits until the GPU finished writing it: the
/// syncobj acquire point if the client set one, else the dmabuf's implicit
/// fence. Installed on every new surface.
pub fn install_blockers(surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) {
    add_pre_commit_hook::<Ede, _>(surface, |state, _dh, surface| {
        #[cfg(feature = "udev")]
        let mut acquire = None;
        let dmabuf = with_states(surface, |data| {
            #[cfg(feature = "udev")]
            acquire.clone_from(&data.cached_state.get::<smithay::wayland::drm_syncobj::DrmSyncobjCachedState>().pending().acquire_point);
            data.cached_state.get::<SurfaceAttributes>().pending().buffer.as_ref().and_then(|b| match b {
                BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                _ => None,
            })
        });
        let Some(dmabuf) = dmabuf else { return };
        let Some(client) = surface.client() else { return };
        #[cfg(feature = "udev")]
        if let Some(point) = acquire {
            if let Ok((blocker, source)) = point.generate_blocker() {
                let c = client.clone();
                let inserted = state.loop_handle.insert_source(source, move |_, _, state| {
                    let dh = state.display_handle.clone();
                    state.client_compositor_state(&c).blocker_cleared(state, &dh);
                    Ok(())
                });
                if inserted.is_ok() {
                    add_blocker(surface, blocker);
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
