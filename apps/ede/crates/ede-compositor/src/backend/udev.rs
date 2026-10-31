//! The real session: libseat for device access and VT switching, udev for
//! GPU discovery and hotplug, DRM/KMS + GBM + GLES per connector, libinput
//! for input. Outputs line up left to right in connector order; unplugging
//! one moves its windows onto a remaining output.
//!
//! Structure follows smithay's anvil (MIT, Copyright (c) 2017 Victor Berger
//! and Smithay contributors), cut down to what eDE needs today: one render
//! GPU (the primary), no DRM leases, no dmabuf feedback. Explicit sync for
//! NVIDIA comes with Xwayland in #NEW-102.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::Path,
    rc::Rc,
    time::Duration,
};

use smithay::{
    backend::{
        allocator::{
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType,
        },
        egl::{EGLDevice, EGLDisplay},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::{memory::MemoryRenderBuffer, surface::WaylandSurfaceRenderElement},
            gles::GlesRenderer,
            multigpu::{gbm::GbmGlesBackend, GpuManager, MultiRenderer},
            ImportDma, ImportMemWl,
        },
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent},
    },
    output::{Mode as WlMode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop, RegistrationToken,
        },
        drm::{
            control::{connector, crtc, Device as _, ModeTypeFlags},
            Device as _,
        },
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::{backend::GlobalId, Display},
    },
    utils::DeviceFd,
    wayland::{
        dmabuf::DmabufFeedbackBuilder,
        drm_syncobj::{supports_syncobj_eventfd, DrmSyncobjState},
    },
};

use super::{cursor::Cursor, OutputElements};
use crate::{config::Config, state::Ede};

/// 10-bit first when the display takes it, else 8-bit.
const COLOR_FORMATS: &[Fourcc] = &[Fourcc::Abgr2101010, Fourcc::Argb2101010, Fourcc::Abgr8888, Fourcc::Argb8888];

type UdevRenderer<'a> = MultiRenderer<'a, 'a, GbmGlesBackend<GlesRenderer, DrmDeviceFd>, GbmGlesBackend<GlesRenderer, DrmDeviceFd>>;
type GbmDrmOutput = DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;
type GbmDrmOutputManager = DrmOutputManager<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

/// Which device + CRTC an [`Output`] is, kept in its user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutputId {
    node: DrmNode,
    crtc: crtc::Handle,
}

struct Surface {
    output: Output,
    connector: connector::Handle,
    drm_output: GbmDrmOutput,
    global: Option<GlobalId>,
    /// A page flip is queued; the next render waits for its vblank.
    queued: bool,
    /// An idle re-check is scheduled (nothing changed last time).
    idle_timer: Option<RegistrationToken>,
}

struct Device {
    manager: GbmDrmOutputManager,
    surfaces: HashMap<crtc::Handle, Surface>,
    render_node: DrmNode,
    token: RegistrationToken,
}

pub struct Udev {
    session: LibSeatSession,
    primary_gpu: DrmNode,
    gpus: GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>,
    devices: HashMap<DrmNode, Device>,
    cursor: Cursor,
    cursor_buffers: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
}

type Shared = Rc<RefCell<Udev>>;

/// Runs eDE as the session's compositor until the loop stops; `autostart`
/// (if any) starts once clients can connect.
pub fn run(config: Config, autostart: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<'static, Ede> = EventLoop::try_new()?;
    let display: Display<Ede> = Display::new()?;

    let (session, notifier) = LibSeatSession::new().map_err(|e| format!("no seat session (run it from a TTY or a login manager): {e}"))?;
    let seat_name = session.seat();

    let primary_gpu = match std::env::var("EDE_DRM_DEVICE") {
        Ok(path) => DrmNode::from_path(path)?,
        Err(_) => primary_gpu(&seat_name)?
            .and_then(|p| DrmNode::from_path(p).ok()?.node_with_type(NodeType::Render)?.ok())
            .or_else(|| all_gpus(&seat_name).ok()?.into_iter().find_map(|p| DrmNode::from_path(p).ok()))
            .ok_or("no GPU found")?,
    };
    tracing::info!("primary GPU {primary_gpu}");

    let mut state = Ede::new(display, event_loop.handle(), event_loop.get_signal(), config);
    let socket = state.listen()?;
    tracing::info!("listening on {}", socket.to_string_lossy());
    std::env::set_var("WAYLAND_DISPLAY", &socket);
    if !autostart.is_empty() {
        state.autostart.push(autostart);
    }

    let udev = Rc::new(RefCell::new(Udev {
        session: session.clone(),
        primary_gpu,
        gpus: GpuManager::new(GbmGlesBackend::default())?,
        devices: HashMap::new(),
        cursor: Cursor::load(),
        cursor_buffers: Vec::new(),
    }));

    // Input.
    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput.udev_assign_seat(&seat_name).map_err(|()| "libinput could not take the seat")?;
    event_loop.handle().insert_source(LibinputInputBackend::new(libinput.clone()), move |event, _, state| {
        if !matches!(event, InputEvent::DeviceAdded { .. } | InputEvent::DeviceRemoved { .. }) {
            state.process_input_event(event);
        }
    })?;

    // VT switches: pause the DRM devices and libinput while away.
    let u = udev.clone();
    event_loop.handle().insert_source(notifier, move |event, _, state| match event {
        SessionEvent::PauseSession => {
            tracing::info!("session paused");
            libinput.suspend();
            for device in u.borrow_mut().devices.values_mut() {
                device.manager.pause();
            }
        }
        SessionEvent::ActivateSession => {
            tracing::info!("session resumed");
            if let Err(e) = libinput.resume() {
                tracing::error!("libinput resume failed: {e:?}");
            }
            let nodes: Vec<_> = {
                let mut udev = u.borrow_mut();
                for device in udev.devices.values_mut() {
                    if let Err(e) = device.manager.activate(false) {
                        tracing::error!("DRM resume failed: {e}");
                    }
                    for s in device.surfaces.values_mut() {
                        s.queued = false;
                    }
                }
                udev.devices.keys().copied().collect()
            };
            for node in nodes {
                render_device(&u, state, node);
            }
        }
    })?;

    // GPUs: the primary first, so outputs on other cards can render through it.
    let udev_backend = UdevBackend::new(&seat_name)?;
    let mut devices: Vec<_> = udev_backend.device_list().map(|(id, path)| (id, path.to_path_buf())).collect();
    let primary_id = primary_gpu.node_with_type(NodeType::Primary).and_then(|n| n.ok()).map(|n| n.dev_id());
    devices.sort_by_key(|(id, _)| *id != primary_gpu.dev_id() && primary_id != Some(*id));
    for (id, path) in devices {
        match DrmNode::from_dev_id(id) {
            Ok(node) => {
                if let Err(e) = device_added(&udev, &mut state, node, &path) {
                    tracing::warn!("skipping {}: {e}", path.display());
                }
            }
            Err(e) => tracing::warn!("skipping {}: {e}", path.display()),
        }
    }
    if udev.borrow().devices.is_empty() {
        // Typically another display server is the DRM master (eDE started
        // from inside a running session): nothing to draw on, so stop here
        // instead of holding the seat's input devices.
        return Err("no GPU could be opened; is another display server running on this seat? (nested: run inside X11/Wayland without --udev)".into());
    }
    {
        let mut u = udev.borrow_mut();
        let primary = u.primary_gpu;
        if let Ok(renderer) = u.gpus.single_renderer(&primary) {
            state.shm_state.update_formats(renderer.shm_formats());
            // linux-dmabuf v4 on the primary GPU's formats.
            let formats = renderer.dmabuf_formats();
            match DmabufFeedbackBuilder::new(primary.dev_id(), formats).build() {
                Ok(feedback) => {
                    state.dmabuf_state.create_global_with_default_feedback::<Ede>(&state.display_handle, &feedback);
                }
                Err(e) => tracing::warn!("no dmabuf feedback ({e}); GPU clients fall back to shm"),
            }
        }
        // Explicit sync when the primary GPU's DRM fd supports syncobj eventfds
        // (NVIDIA needs it for flicker-free Xwayland and games).
        let primary_node = primary.node_with_type(NodeType::Primary).and_then(|n| n.ok());
        let fd = primary_node.and_then(|n| u.devices.get(&n)).map(|d| d.manager.device().device_fd().clone());
        match fd {
            Some(fd) if supports_syncobj_eventfd(&fd) => {
                state.syncobj_state = Some(DrmSyncobjState::new::<Ede>(&state.display_handle, fd));
                tracing::info!("explicit sync (drm-syncobj) on");
            }
            _ => tracing::info!("explicit sync (drm-syncobj) unavailable on this GPU/kernel"),
        }
    }
    let mut session_for_vt = session.clone();
    state.vt_switch = Some(Box::new(move |vt| {
        if let Err(e) = session_for_vt.change_vt(vt) {
            tracing::warn!("VT {vt}: {e}");
        }
    }));
    let importer = udev.clone();
    state.dmabuf_importer = Some(Box::new(move |dmabuf| {
        let mut u = importer.borrow_mut();
        let primary = u.primary_gpu;
        u.gpus.single_renderer(&primary).is_ok_and(|mut r| r.import_dmabuf(dmabuf, None).is_ok())
    }));
    let u = udev.clone();
    event_loop.handle().insert_source(udev_backend, move |event, _, state| match event {
        UdevEvent::Added { device_id, path } => {
            if let Ok(node) = DrmNode::from_dev_id(device_id) {
                if let Err(e) = device_added(&u, state, node, &path) {
                    tracing::warn!("skipping {}: {e}", path.display());
                }
            }
        }
        UdevEvent::Changed { device_id } => {
            if let Ok(node) = DrmNode::from_dev_id(device_id) {
                scan_connectors(&u, state, node);
            }
        }
        UdevEvent::Removed { device_id } => {
            if let Ok(node) = DrmNode::from_dev_id(device_id) {
                device_removed(&u, state, node);
            }
        }
    })?;

    state.start_session();

    // Every loop turn: repaint outputs that are not waiting for a vblank.
    // Clean frames submit nothing, so an idle desktop stays idle.
    let u = udev.clone();
    event_loop.run(None, &mut state, move |state| {
        let nodes: Vec<_> = u.borrow().devices.keys().copied().collect();
        for node in nodes {
            render_device(&u, state, node);
        }
        let _ = state.display_handle.flush_clients();
    })?;
    Ok(())
}

fn device_added(udev: &Shared, state: &mut Ede, node: DrmNode, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut u = udev.borrow_mut();
    let fd = u.session.open(path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier) = DrmDevice::new(fd.clone(), true)?;
    let gbm = GbmDevice::new(fd)?;

    // One render GPU: the device's own render node if it has a working EGL
    // device, else the primary GPU renders for it.
    let display = unsafe { EGLDisplay::new(gbm.clone())? };
    let egl_device = EGLDevice::device_for_display(&display)?;
    let render_node = egl_device.try_get_render_node().ok().flatten().unwrap_or(node);
    u.gpus.as_mut().add_node(render_node, gbm.clone())?;

    let allocator = GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let exporter = GbmFramebufferExporter::new(gbm.clone(), Some(render_node));
    let render_formats = u
        .gpus
        .single_renderer(&render_node)?
        .as_mut()
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect::<FormatSet>();
    let manager = DrmOutputManager::new(drm, allocator, exporter, Some(gbm), COLOR_FORMATS.iter().copied(), render_formats);

    let ud = udev.clone();
    let token = state.loop_handle.insert_source(notifier, move |event, _, state| match event {
        DrmEvent::VBlank(crtc) => vblank(&ud, state, node, crtc),
        DrmEvent::Error(e) => tracing::error!("DRM error on {node}: {e:?}"),
    })?;

    u.devices.insert(node, Device { manager, surfaces: HashMap::new(), render_node, token });
    drop(u);
    scan_connectors(udev, state, node);
    Ok(())
}

fn device_removed(udev: &Shared, state: &mut Ede, node: DrmNode) {
    let Some(mut device) = udev.borrow_mut().devices.remove(&node) else { return };
    for (_, surface) in device.surfaces.drain() {
        drop_surface(state, surface);
    }
    state.loop_handle.remove(device.token);
    udev.borrow_mut().gpus.as_mut().remove_node(&device.render_node);
}

/// Connects outputs for newly connected connectors and drops the ones that
/// went away (hotplug).
fn scan_connectors(udev: &Shared, state: &mut Ede, node: DrmNode) {
    let mut u = udev.borrow_mut();
    let u = &mut *u;
    let Some(device) = u.devices.get_mut(&node) else { return };
    let drm = device.manager.device();
    let Ok(resources) = drm.resource_handles() else { return };
    let infos: Vec<connector::Info> = resources.connectors().iter().filter_map(|c| drm.get_connector(*c, true).ok()).collect();

    // Gone: connector no longer connected.
    let gone: Vec<crtc::Handle> = device
        .surfaces
        .iter()
        .filter(|(_, s)| !infos.iter().any(|i| i.handle() == s.connector && i.state() == connector::State::Connected))
        .map(|(c, _)| *c)
        .collect();
    for crtc in gone {
        if let Some(surface) = device.surfaces.remove(&crtc) {
            tracing::info!("output {} disconnected", surface.output.name());
            drop_surface(state, surface);
        }
    }

    // New: connected, not set up yet, with a free CRTC its encoders reach.
    for info in infos.iter().filter(|i| i.state() == connector::State::Connected) {
        if device.surfaces.values().any(|s| s.connector == info.handle()) || info.modes().is_empty() {
            continue;
        }
        let drm = device.manager.device();
        let used: Vec<crtc::Handle> = device.surfaces.keys().copied().collect();
        let crtc = info
            .encoders()
            .iter()
            .filter_map(|e| drm.get_encoder(*e).ok())
            .flat_map(|enc| resources.filter_crtcs(enc.possible_crtcs()))
            .find(|c| !used.contains(c));
        let Some(crtc) = crtc else {
            tracing::warn!("no free CRTC for {:?}", info.interface());
            continue;
        };

        let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
        let mode = info.modes().iter().find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED)).copied().unwrap_or(info.modes()[0]);
        let wl_mode = WlMode::from(mode);
        let (mm_w, mm_h) = info.size().unwrap_or((0, 0));
        let output = Output::new(
            name.clone(),
            PhysicalProperties { size: (mm_w as i32, mm_h as i32).into(), subpixel: Subpixel::Unknown, make: "Unknown".into(), model: name.clone() },
        );
        let global = output.create_global::<Ede>(&state.display_handle);
        output.set_preferred(wl_mode);
        output.change_current_state(Some(wl_mode), None, None, None);
        output.user_data().insert_if_missing(|| OutputId { node, crtc });

        let mut planes = match drm.planes(&crtc) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("{name}: no planes: {e}");
                continue;
            }
        };
        // Overlay planes break on NVIDIA's driver (anvil found the same).
        let nvidia = drm.get_driver().map(|d| d.name().to_string_lossy().to_lowercase().contains("nvidia")).unwrap_or(false);
        if nvidia {
            planes.overlay.clear();
        }

        let Ok(mut renderer) = u.gpus.single_renderer(&device.render_node) else { continue };
        let drm_output = match device.manager.initialize_output::<_, OutputElements<UdevRenderer<'_>, WaylandSurfaceRenderElement<UdevRenderer<'_>>>>(
            crtc,
            mode,
            &[info.handle()],
            &output,
            Some(planes),
            &mut renderer,
            &DrmOutputRenderElements::default(),
        ) {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("{name}: could not drive it: {e}");
                state.display_handle.remove_global::<Ede>(global);
                continue;
            }
        };
        tracing::info!("output {name} connected, {}x{}@{}", wl_mode.size.w, wl_mode.size.h, wl_mode.refresh);
        state.add_output(&output);
        device.surfaces.insert(crtc, Surface { output, connector: info.handle(), drm_output, global: Some(global), queued: false, idle_timer: None });
    }
}

fn drop_surface(state: &mut Ede, mut surface: Surface) {
    state.remove_output(&surface.output);
    if let Some(global) = surface.global.take() {
        state.display_handle.disable_global::<Ede>(global);
    }
    if let Some(t) = surface.idle_timer.take() {
        state.loop_handle.remove(t);
    }
}

fn vblank(udev: &Shared, state: &mut Ede, node: DrmNode, crtc: crtc::Handle) {
    {
        let mut u = udev.borrow_mut();
        let Some(surface) = u.devices.get_mut(&node).and_then(|d| d.surfaces.get_mut(&crtc)) else { return };
        if let Err(e) = surface.drm_output.frame_submitted() {
            tracing::warn!("frame_submitted: {e:?}");
        }
        surface.queued = false;
    }
    render_device(udev, state, node);
}

fn render_device(udev: &Shared, state: &mut Ede, node: DrmNode) {
    let crtcs: Vec<crtc::Handle> = match udev.borrow().devices.get(&node) {
        Some(d) => d.surfaces.iter().filter(|(_, s)| !s.queued).map(|(c, _)| *c).collect(),
        None => return,
    };
    for crtc in crtcs {
        render_surface(udev, state, node, crtc);
    }
}

fn render_surface(udev: &Shared, state: &mut Ede, node: DrmNode, crtc: crtc::Handle) {
    let mut u = udev.borrow_mut();
    let u = &mut *u;
    if !u.session.is_active() {
        return;
    }
    let primary = u.primary_gpu;
    let Some(device) = u.devices.get_mut(&node) else { return };
    let Some(surface) = device.surfaces.get_mut(&crtc) else { return };
    let output = surface.output.clone();
    let Some(output_geo) = state.space.output_geometry(&output) else { return };

    let render_node = device.render_node;
    let renderer = if render_node == primary {
        u.gpus.single_renderer(&render_node)
    } else {
        let format = surface.drm_output.format();
        u.gpus.renderer(&primary, &render_node, format)
    };
    let mut renderer = match renderer {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("no renderer for {node}: {e}");
            return;
        }
    };

    // The cursor: the client's surface if it set one, else the theme's arrow.
    let pointer = state.seat.get_pointer().unwrap().current_location();
    let mut elements: Vec<OutputElements<UdevRenderer<'_>, WaylandSurfaceRenderElement<UdevRenderer<'_>>>> = Vec::new();
    if output_geo.to_f64().contains(pointer) {
        let image = u.cursor.image(state.start_time.elapsed());
        let buffer = match u.cursor_buffers.iter().find(|(i, _)| *i == image) {
            Some((_, b)) => b.clone(),
            None => {
                let b = MemoryRenderBuffer::from_slice(
                    &image.pixels_rgba,
                    Fourcc::Abgr8888,
                    (image.width as i32, image.height as i32),
                    1,
                    smithay::utils::Transform::Normal,
                    None,
                );
                u.cursor_buffers.push((image.clone(), b.clone()));
                b
            }
        };
        elements.extend(super::cursor_elements(state, &mut renderer, pointer - output_geo.loc.to_f64(), &buffer, (image.xhot as i32, image.yhot as i32)));
    }
    elements.extend(super::output_elements(state, &output, &mut renderer));

    let result = surface.drm_output.render_frame(&mut renderer, &elements, state.clear_color(), FrameFlags::DEFAULT);
    drop(renderer);
    match result {
        Ok(frame) if !frame.is_empty => match surface.drm_output.queue_frame(()) {
            Ok(()) => {
                surface.queued = true;
                if let Some(t) = surface.idle_timer.take() {
                    state.loop_handle.remove(t);
                }
                super::frame_done(state, &output);
            }
            Err(e) => tracing::warn!("{}: queue_frame: {e:?}", output.name()),
        },
        Ok(_) => {
            // Nothing changed. Clients still waiting on a frame callback get
            // one a refresh later, so they never stall.
            if surface.idle_timer.is_none() {
                let refresh = output.current_mode().map(|m| m.refresh).filter(|r| *r > 0).unwrap_or(60_000);
                let ud = udev.clone();
                let o = output.clone();
                surface.idle_timer = state
                    .loop_handle
                    .insert_source(Timer::from_duration(Duration::from_micros(1_000_000_000 / refresh as u64)), move |_, _, state| {
                        if let Some(s) = ud.borrow_mut().devices.get_mut(&node).and_then(|d| d.surfaces.get_mut(&crtc)) {
                            s.idle_timer = None;
                        }
                        super::frame_done(state, &o);
                        TimeoutAction::Drop
                    })
                    .ok();
            }
        }
        Err(e) => tracing::warn!("{}: render failed: {e:?}", output.name()),
    }
}
