//! The real session: libseat for device access and VT switching, udev for
//! GPU discovery and hotplug, DRM/KMS + GBM + GLES per connector, libinput
//! for input. Outputs line up left to right in connector order; unplugging
//! one moves its windows onto a remaining output.
//!
//! Structure follows smithay's anvil (MIT, Copyright (c) 2017 Victor Berger
//! and Smithay contributors), cut down to what eDE needs today: one render
//! GPU (the primary), no DRM leases. Explicit sync for NVIDIA comes with
//! Xwayland in #NEW-102. #EDE-18: the mode from `[[outputs]]`, a live
//! modeset through [`Ede::set_mode`], and a per-output dmabuf feedback
//! with a scanout tranche of the primary plane's formats, sent to a
//! fullscreen window so games and video allocate buffers the DRM
//! compositor can put straight on the plane (direct scanout). A connector
//! that changes while connected (a VM window resized: VirtualBox's
//! VBoxDRMClient and VMware push a new preferred mode through vmwgfx and
//! raise a hotplug; a monitor re-reading its EDID) follows its new
//! preferred mode unless `[[outputs]]` pins one it still offers. Frames on
//! a GPU without IN_FENCE_FD (vmwgfx, virtio-gpu, llvmpipe) wait for the
//! renderer before the flip, else the host shows them half drawn.

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
            compositor::{FrameFlags, PrimaryPlaneElement},
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType,
        },
        egl::{EGLDevice, EGLDisplay},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::memory::MemoryRenderBuffer,
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
            control::{connector, crtc, Device as _, Mode, ModeTypeFlags},
            ClientCapability, Device as _,
        },
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::{backend::GlobalId, Display},
    },
    utils::{DeviceFd, Physical, Point},
    wayland::{
        dmabuf::{DmabufFeedback, DmabufFeedbackBuilder},
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
    /// What a fullscreen window hears: render formats, then the primary
    /// plane's formats as a scanout tranche (`None` = no feedback).
    feedback: Option<(DmabufFeedback, DmabufFeedback)>,
    /// The HOTSPOT_X/Y last given to the cursor planes (virtual GPUs only).
    hotspot: Option<Point<i32, Physical>>,
}

struct Device {
    manager: GbmDrmOutputManager,
    surfaces: HashMap<crtc::Handle, Surface>,
    render_node: DrmNode,
    token: RegistrationToken,
    /// A virtual GPU that took DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT: its
    /// cursor plane is the host's cursor, placed by its HOTSPOT_X/Y.
    cursor_hotspot: bool,
    /// A connector scan is due ([`RESCAN_DELAY`] after the last change).
    rescan_timer: Option<RegistrationToken>,
}

/// Connector changes come in bursts (a VM window being dragged pushes a new
/// layout every 250 ms, a monitor waking re-reads its EDID): one scan this
/// long after the last change, so a drag ends in one modeset.
const RESCAN_DELAY: Duration = Duration::from_millis(150);

pub struct Udev {
    session: LibSeatSession,
    primary_gpu: DrmNode,
    gpus: GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>,
    devices: HashMap<DrmNode, Device>,
    cursor: Cursor,
    cursor_buffers: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    /// Whether the outputs ever drew (#EDE-40).
    health: super::FrameHealth,
    /// Safe mode: software rendering on a real GPU is tolerated (#EDE-59),
    /// so a broken driver still gets a desktop to fix it from.
    safe_mode: bool,
}

type Shared = Rc<RefCell<Udev>>;

/// Runs eDE as the session's compositor until the loop stops; `autostart`
/// (if any) starts once clients can connect. `safe_mode` = the stock
/// settings run (`--safe-mode`, `EDE_SAFE_MODE`): software rendering is
/// tolerated on any GPU.
pub fn run(config: Config, config_path: Option<std::path::PathBuf>, autostart: Vec<String>, safe_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<'static, Ede> = EventLoop::try_new()?;
    let display: Display<Ede> = Display::new()?;

    let (session, notifier) = LibSeatSession::new().map_err(|e| format!("no seat session (run it from a TTY or a login manager): {e}"))?;
    let seat_name = session.seat();

    // The GPU everything renders on: `EDE_DRM_DEVICE`, else the card with
    // the displays (#EDE-59: a desktop whose monitors hang off the discrete
    // card while the firmware booted on the iGPU renders on the discrete
    // one, not through a copy per frame), else udev's boot GPU, else any.
    let cards = crate::doctor::read_drm_cards(Path::new("/sys/class/drm"));
    let primary_gpu = match std::env::var("EDE_DRM_DEVICE") {
        Ok(path) => {
            let node = DrmNode::from_path(path)?;
            super::render_node_for(None, node.node_with_type(NodeType::Render).and_then(Result::ok), node)
        }
        Err(_) => crate::doctor::primary_card(&cards)
            .and_then(|c| DrmNode::from_path(Path::new("/dev/dri").join(&c.name)).ok()?.node_with_type(NodeType::Render)?.ok())
            .or_else(|| primary_gpu(&seat_name).ok().flatten().and_then(|p| DrmNode::from_path(p).ok()?.node_with_type(NodeType::Render)?.ok()))
            .or_else(|| all_gpus(&seat_name).ok()?.into_iter().find_map(|p| DrmNode::from_path(p).ok()))
            .ok_or("no GPU found")?,
    };
    match crate::doctor::primary_card(&cards) {
        Some(card) => tracing::info!(
            "primary GPU {primary_gpu} ({} {}{}: {})",
            card.name,
            card.driver.as_deref().unwrap_or("unknown driver"),
            if card.boot_vga { ", boot GPU" } else { "" },
            if card.connected.is_empty() { "nothing connected".to_string() } else { card.connected.join(", ") }
        ),
        None => tracing::info!("primary GPU {primary_gpu}"),
    }

    let mut state = Ede::new(display, event_loop.handle(), event_loop.get_signal(), config);
    state.config_path = config_path;
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
        health: super::FrameHealth::default(),
        safe_mode,
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
                // Back from a VT switch or a suspend: every output draws a
                // whole first frame from fresh buffers, and the cursor
                // images are uploaded again. On NVIDIA without
                // NVreg_PreserveVideoMemoryAllocations the GPU forgot its
                // memory while asleep (#EDE-59; the doctor's "NVIDIA
                // sleep" row), so nothing cached before is trusted.
                udev.cursor_buffers.clear();
                for device in udev.devices.values_mut() {
                    if let Err(e) = device.manager.lock().activate(false) {
                        tracing::error!("DRM resume failed: {e}");
                    }
                    for s in device.surfaces.values_mut() {
                        s.drm_output.reset_buffers();
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
    let mut failures: Vec<String> = Vec::new();
    for (id, path) in devices {
        let added = DrmNode::from_dev_id(id).map_err(|e| e.to_string()).and_then(|node| device_added(&udev, &mut state, node, &path).map_err(|e| e.to_string()));
        if let Err(e) = added {
            tracing::warn!("skipping {}: {e}", path.display());
            failures.push(format!("{}: {e}", path.display()));
        }
    }
    if udev.borrow().devices.is_empty() {
        // Nothing to draw on: stop here with every card's reason instead of
        // holding the seat's input devices (#EDE-59: a GPU whose driver
        // stack is broken says so on the recovery screen).
        return Err(super::no_gpu_message(&failures).into());
    }
    {
        let mut u = udev.borrow_mut();
        // The primary GPU must be a node a renderer was registered under
        // (#EDE-40: on a software EGL device the card's render node is not
        // what EGL names), else every frame would ask for a renderer that
        // does not exist.
        let registered: Vec<DrmNode> = u.devices.values().map(|d| d.render_node).collect();
        if let Some(fallback) = super::reconcile_primary(u.primary_gpu, &registered) {
            tracing::warn!("primary GPU {} has no renderer; rendering through {fallback}", u.primary_gpu);
            u.primary_gpu = fallback;
        }
        let primary = u.primary_gpu;
        match u.gpus.single_renderer(&primary) {
            Ok(renderer) => {
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
            Err(e) => return Err(format!("no renderer on the primary GPU {primary}: {e}").into()),
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
    // A modeset for `set_output` (#EDE-18).
    let modeset = udev.clone();
    state.set_mode = Some(Box::new(move |output, wl_mode| {
        let id = *output.user_data().get::<OutputId>().ok_or("not a DRM output")?;
        let mut u = modeset.borrow_mut();
        let u = &mut *u;
        let device = u.devices.get_mut(&id.node).ok_or("its GPU is gone")?;
        let surface = device.surfaces.get_mut(&id.crtc).ok_or("its output is gone")?;
        let info = device.manager.device().get_connector(surface.connector, false).map_err(|e| e.to_string())?;
        let mode = info
            .modes()
            .iter()
            .find(|m| WlMode::from(**m) == wl_mode)
            .copied()
            .ok_or_else(|| format!("{} has no mode {}x{}@{}", output.name(), wl_mode.size.w, wl_mode.size.h, wl_mode.refresh))?;
        let mut renderer = u.gpus.single_renderer(&device.render_node).map_err(|e| e.to_string())?;
        surface.drm_output.use_mode(mode, &mut renderer, &DrmOutputRenderElements::<_, OutputElements<UdevRenderer<'_>>>::default()).map_err(|e| format!("{e:?}"))?;
        output.change_current_state(Some(wl_mode), None, None, None);
        surface.queued = false;
        tracing::info!("output {} now {}x{}@{}", output.name(), wl_mode.size.w, wl_mode.size.h, wl_mode.refresh);
        Ok(())
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
                schedule_scan(&u, state, node);
            }
        }
        UdevEvent::Removed { device_id } => {
            if let Ok(node) = DrmNode::from_dev_id(device_id) {
                device_removed(&u, state, node);
            }
        }
    })?;

    crate::logind::start(&mut state);
    state.start_session();
    // A wake-up each second: the recovery screen (#EDE-16) appears on an
    // idle desktop without waiting for input.
    event_loop.handle().insert_source(Timer::from_duration(Duration::from_secs(1)), |_, _, _| TimeoutAction::ToDuration(Duration::from_secs(1)))?;

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
    match state.fatal.take() {
        Some(reason) => Err(reason.into()),
        None => Ok(()),
    }
}

fn device_added(udev: &Shared, state: &mut Ede, node: DrmNode, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut u = udev.borrow_mut();
    let fd = u.session.open(path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    // A virtual GPU (vmwgfx, virtio-gpu, qxl, vboxvideo) hides its cursor
    // plane from atomic clients that do not set its hotspot, so the cursor
    // was drawn into the frame while VirtualBox's mouse integration showed
    // the host's own pointer as well: two cursors. Taking the cap (it needs
    // atomic first; DrmDevice::new maps the planes it then sees) puts the
    // cursor on that plane, which the host draws as its one pointer.
    // Real GPUs refuse the cap and keep their cursor plane as it was.
    let cursor_hotspot = fd.set_client_capability(ClientCapability::Atomic, true).is_ok()
        && fd.set_client_capability(ClientCapability::CursorPlaneHotspot, true).is_ok();
    if cursor_hotspot {
        tracing::info!("{node}: virtual GPU, the cursor plane carries the hotspot (one host cursor)");
    }
    let (drm, notifier) = DrmDevice::new(fd.clone(), true)?;
    // What this card's KMS offers, in the journal (#EDE-59: the first run on
    // real hardware tells whether atomic, the cursor plane, modifiers and
    // explicit sync are there). Legacy KMS is never silent: no cursor
    // plane, no direct scanout, and on NVIDIA it means modeset is off.
    let kms = crate::doctor::KmsProbe::read(&fd);
    let driver = kms.as_ref().map(|k| k.driver.clone()).unwrap_or_default();
    match &kms {
        Ok(probe) => tracing::info!("{node} {}", probe.describe()),
        Err(e) => tracing::warn!("{node}: KMS capabilities unreadable: {e}"),
    }
    if !drm.is_atomic() {
        if driver.contains("nvidia") {
            return Err("nvidia-drm has no atomic modesetting: nvidia-drm.modeset=1 is not in effect (run `exponential-compositor doctor`)".into());
        }
        tracing::warn!("{node} ({driver}): legacy KMS only; no cursor plane, no direct scanout");
    }
    let gbm = GbmDevice::new(fd).map_err(|e| format!("GBM on {driver}: {e}"))?;

    // One render GPU: the node EGL's device names, else the card's own
    // render node (Mesa's software device names none: llvmpipe on vmwgfx
    // without 3D, a firmware framebuffer; #EDE-40), else the card itself.
    let display = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| {
        let hint = if driver.contains("nvidia") { " (the driver's GBM backend nvidia-drm_gbm.so or its EGL GBM platform is missing; run `exponential-compositor doctor`)" } else { "" };
        format!("EGL on {driver}'s GBM device failed: {e}{hint}")
    })?;
    let egl_device = EGLDevice::device_for_display(&display)?;
    let egl_node = egl_device.try_get_render_node().ok().flatten();
    let render_node = super::render_node_for(egl_node, node.node_with_type(NodeType::Render).and_then(Result::ok), node);
    if egl_device.is_software() {
        // A real GPU in software is a broken driver, never a desktop to
        // keep quiet about (#EDE-59): refused unless this is safe mode.
        match crate::doctor::software_rendering(&driver, u.safe_mode) {
            Ok(note) => tracing::warn!("{node}: {note}; rendering through {render_node}"),
            Err(reason) => return Err(reason.into()),
        }
    }
    u.gpus.as_mut().add_node(render_node, gbm.clone())?;

    let allocator = GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let exporter = GbmFramebufferExporter::new(gbm.clone(), render_node.into());
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

    u.devices.insert(node, Device { manager, surfaces: HashMap::new(), render_node, token, cursor_hotspot, rescan_timer: None });
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
    if let Some(t) = device.rescan_timer.take() {
        state.loop_handle.remove(t);
    }
    udev.borrow_mut().gpus.as_mut().remove_node(&device.render_node);
}

/// Scans `node`'s connectors [`RESCAN_DELAY`] after the last change event.
fn schedule_scan(udev: &Shared, state: &mut Ede, node: DrmNode) {
    let mut u = udev.borrow_mut();
    let Some(device) = u.devices.get_mut(&node) else { return };
    if let Some(t) = device.rescan_timer.take() {
        state.loop_handle.remove(t);
    }
    let ud = udev.clone();
    device.rescan_timer = state
        .loop_handle
        .insert_source(Timer::from_duration(RESCAN_DELAY), move |_, _, state| {
            if let Some(d) = ud.borrow_mut().devices.get_mut(&node) {
                d.rescan_timer = None;
            }
            scan_connectors(&ud, state, node);
            TimeoutAction::Drop
        })
        .ok();
}

/// The connector's preferred mode (the first one when it marks none).
fn preferred_mode(info: &connector::Info) -> Mode {
    info.modes().iter().find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED)).copied().unwrap_or(info.modes()[0])
}

/// The connector's mode `[[outputs]]` asks for (`refresh` 0 = the highest
/// of that size), if it offers one.
fn configured_drm_mode(info: &connector::Info, (w, h, refresh): (i32, i32, i32)) -> Option<Mode> {
    let mut fitting: Vec<Mode> = info.modes().iter().filter(|m| m.size() == (w as u16, h as u16)).copied().collect();
    fitting.sort_by_key(|m| std::cmp::Reverse(m.vrefresh()));
    if refresh == 0 {
        fitting.first().copied()
    } else {
        fitting.into_iter().find(|m| (WlMode::from(*m).refresh - refresh).abs() < 500)
    }
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

    // Still connected, but its modes changed: follow the new preferred
    // mode (a VM window resized, a monitor's new EDID) unless `[[outputs]]`
    // pins a mode the connector still offers.
    let mut resized: Vec<Output> = Vec::new();
    for surface in device.surfaces.values_mut() {
        let Some(info) = infos.iter().find(|i| i.handle() == surface.connector) else { continue };
        if info.modes().is_empty() {
            continue;
        }
        let name = surface.output.name();
        let preferred = preferred_mode(info);
        let pinned = state.configured_mode(&name).and_then(|wanted| configured_drm_mode(info, wanted));
        let Some(wanted) = super::mode_to_follow(surface.output.current_mode(), WlMode::from(preferred), pinned.map(WlMode::from)) else { continue };
        let Some(mode) = info.modes().iter().find(|m| WlMode::from(**m) == wanted).copied() else { continue };
        let Ok(mut renderer) = u.gpus.single_renderer(&device.render_node) else { continue };
        let set = surface.drm_output.use_mode(mode, &mut renderer, &DrmOutputRenderElements::<_, OutputElements<UdevRenderer<'_>>>::default());
        drop(renderer);
        if let Err(e) = set {
            tracing::warn!("{name}: could not switch to {}x{}@{}: {e:?}", wanted.size.w, wanted.size.h, wanted.refresh);
            continue;
        }
        // wl_output's list follows the connector's; the current mode stays listed.
        let offered: Vec<WlMode> = info.modes().iter().map(|m| WlMode::from(*m)).collect();
        for m in surface.output.modes() {
            if !offered.contains(&m) && m != wanted {
                surface.output.delete_mode(m);
            }
        }
        for m in &offered {
            surface.output.add_mode(*m);
        }
        surface.output.set_preferred(WlMode::from(preferred));
        surface.output.change_current_state(Some(wanted), None, None, None);
        surface.queued = false;
        tracing::info!("output {name} now {}x{}@{} (its preferred mode changed)", wanted.size.w, wanted.size.h, wanted.refresh);
        resized.push(surface.output.clone());
    }
    for output in &resized {
        state.output_resized(output);
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
        let preferred = preferred_mode(info);
        // `[[outputs]]` may name a mode (refresh 0 = the highest of that size).
        let mode = match state.configured_mode(&name) {
            Some(wanted @ (w, h, refresh)) => configured_drm_mode(info, wanted).unwrap_or_else(|| {
                tracing::warn!("{name}: no mode {w}x{h}@{}; using the preferred one", refresh as f64 / 1000.0);
                preferred
            }),
            None => preferred,
        };
        let wl_mode = WlMode::from(mode);
        let (mm_w, mm_h) = info.size().unwrap_or((0, 0));
        let output = Output::new(
            name.clone(),
            PhysicalProperties { size: (mm_w as i32, mm_h as i32).into(), subpixel: Subpixel::Unknown, make: "Unknown".into(), model: name.clone(), serial_number: "Unknown".into() },
        );
        let global = output.create_global::<Ede>(&state.display_handle);
        for m in info.modes() {
            output.add_mode(WlMode::from(*m));
        }
        output.set_preferred(WlMode::from(preferred));
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
        let atomic = drm.is_atomic();
        if nvidia {
            planes.overlay.clear();
        }

        let Ok(mut renderer) = u.gpus.single_renderer(&device.render_node) else { continue };
        let drm_output = match device.manager.lock().initialize_output::<_, OutputElements<UdevRenderer<'_>>>(
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
        // Direct scanout: what the primary plane takes, as a tranche after
        // the render formats; a fullscreen window hears it (render_surface).
        let feedback = {
            let render_formats = renderer.dmabuf_formats();
            let plane_formats = drm_output.with_compositor(|c| c.surface().plane_info().formats.clone());
            let (cursor_planes, overlay_planes) = drm_output.with_compositor(|c| (c.surface().planes().cursor.len(), c.surface().planes().overlay.len()));
            tracing::info!(
                "output {name}: {} ({}), primary plane takes {} formats ({} with modifiers), {cursor_planes} cursor and {overlay_planes} overlay planes in use",
                drm_output.format(),
                if atomic { "atomic" } else { "legacy" },
                plane_formats.iter().count(),
                plane_formats.iter().filter(|f| !matches!(f.modifier, smithay::backend::allocator::Modifier::Invalid | smithay::backend::allocator::Modifier::Linear)).count()
            );
            let render = DmabufFeedbackBuilder::new(device.render_node.dev_id(), render_formats.clone()).build().ok();
            let scanout = DmabufFeedbackBuilder::new(device.render_node.dev_id(), render_formats)
                .add_preference_tranche(node.dev_id(), smithay::reexports::wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags::Scanout, plane_formats, 4u32..=u32::MAX)
                .build()
                .ok();
            render.zip(scanout)
        };
        drop(renderer);
        state.add_output(&output);
        device.surfaces.insert(crtc, Surface { output, connector: info.handle(), drm_output, global: Some(global), queued: false, idle_timer: None, feedback, hotspot: None });
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

/// Whether an output renders on this loop turn: not while a flip waits for
/// its vblank, and not while an idle refresh is pending (#EDE-65: the last
/// render found nothing new, so its frame callbacks go out a refresh later
/// and that timer renders next). Every gpui surface commits once per frame
/// callback even when it draws nothing, and each commit is a loop turn: an
/// idle desktop ran a full render attempt per commit (~240/s with the three
/// bars and the app) instead of one per refresh.
fn wants_render(queued: bool, idle_pending: bool) -> bool {
    !queued && !idle_pending
}

fn render_device(udev: &Shared, state: &mut Ede, node: DrmNode) {
    let crtcs: Vec<crtc::Handle> = match udev.borrow().devices.get(&node) {
        Some(d) => d.surfaces.iter().filter(|(_, s)| wants_render(s.queued, s.idle_timer.is_some())).map(|(c, _)| *c).collect(),
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
            frame_failed(u, state, &output, format!("no renderer for {node}: {e}"));
            return;
        }
    };

    // The cursor: the client's surface if it set one, else the theme's
    // arrow at this output's scale.
    let pointer = state.seat.get_pointer().unwrap().current_location();
    let scale = output.current_scale().fractional_scale();
    let mut elements: Vec<OutputElements<UdevRenderer<'_>>> = Vec::new();
    if output_geo.to_f64().contains(pointer) {
        let image = u.cursor.image(super::cursor_buffer_scale(scale), state.start_time.elapsed());
        let buffer_scale = u.cursor.buffer_scale(&image);
        let buffer = match u.cursor_buffers.iter().find(|(i, _)| *i == image) {
            Some((_, b)) => b.clone(),
            None => {
                let b = MemoryRenderBuffer::from_slice(
                    &image.pixels_rgba,
                    Fourcc::Abgr8888,
                    (image.width as i32, image.height as i32),
                    buffer_scale,
                    smithay::utils::Transform::Normal,
                    None,
                );
                u.cursor_buffers.push((image.clone(), b.clone()));
                b
            }
        };
        let hotspot = Point::from((image.xhot as f64, image.yhot as f64)).downscale(buffer_scale as f64);
        let (cursor, hotspot) = super::cursor_elements(state, &mut renderer, pointer - output_geo.loc.to_f64(), scale, &buffer, hotspot);
        elements.extend(cursor);
        if device.cursor_hotspot && hotspot.is_some() && hotspot != surface.hotspot {
            // No flip is pending here (render_surface only runs unqueued),
            // so a blocking commit of the two properties cannot collide.
            let planes: Vec<_> = surface.drm_output.with_compositor(|c| c.surface().planes().cursor.iter().map(|p| p.handle).collect());
            match set_cursor_hotspot(device.manager.device(), &planes, hotspot.unwrap()) {
                Ok(()) => surface.hotspot = hotspot,
                Err(e) => {
                    tracing::warn!("{}: cursor hotspot: {e}", output.name());
                    device.cursor_hotspot = false;
                }
            }
        }
    }
    elements.extend(super::output_elements(state, &output, &mut renderer));

    let started = std::time::Instant::now();
    let result = surface.drm_output.render_frame(&mut renderer, &elements, state.clear_color(), FrameFlags::DEFAULT);
    drop(renderer);
    match result {
        Ok(frame) if !frame.is_empty => {
            u.health.drew();
            // #EDE-65: what each frame did, for a flicker only a real display
            // shows (RUST_LOG=ede_frames=debug in the session's environment).
            if tracing::enabled!(target: "ede_frames", tracing::Level::DEBUG) {
                let (primary, damage) = match &frame.primary_element {
                    PrimaryPlaneElement::Swapchain(e) => ("composited", e.damage.damage_since(None).map(|d| d.len()).unwrap_or(0)),
                    PrimaryPlaneElement::Element(_) => ("scanout", 0),
                };
                tracing::debug!(target: "ede_frames", output = %output.name(), primary, damage_rects = damage, cursor_plane = frame.cursor_element.is_some(), elements = elements.len(), render_ms = started.elapsed().as_secs_f32() * 1000.0, "frame");
            }
            // No IN_FENCE_FD on this plane (vmwgfx, virtio-gpu, llvmpipe's
            // EGL fences cannot be exported): the kernel would scan out the
            // buffer while the renderer still draws into it, and the host
            // shows half-drawn frames (flicker). Wait for the renderer here,
            // like anvil does.
            if frame.needs_sync() {
                if let PrimaryPlaneElement::Swapchain(element) = &frame.primary_element {
                    let _ = element.sync.wait();
                }
            }
            // The fullscreen window alone on this output may go straight to
            // the plane: tell it which buffers make that possible.
            if let (Some((render_fb, scanout_fb)), Some(w)) = (surface.feedback.as_ref(), state.fullscreen_on(&output)) {
                use smithay::backend::renderer::element::utils::select_dmabuf_feedback;
                w.send_dmabuf_feedback(&output, |_, _| Some(output.clone()), |s, _| select_dmabuf_feedback(s, &frame.states, render_fb, scanout_fb));
            }
            match surface.drm_output.queue_frame(()) {
            Ok(()) => {
                surface.queued = true;
                if let Some(t) = surface.idle_timer.take() {
                    state.loop_handle.remove(t);
                }
                super::frame_done(state, &output);
            }
            Err(e) => {
                // #EDE-65: smithay counted this frame as shown (the slot's
                // age, the elements' damage) though it never reached the
                // screen; vmwgfx copies only the next frames' damage clips to
                // the host, so what changed in it stayed stale on screen.
                // The next frame redraws the whole output instead.
                surface.drm_output.reset_buffers();
                tracing::warn!("{}: queue_frame: {e:?}; the next frame redraws the whole output", output.name());
            }
            }
        }
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
        Err(e) => frame_failed(u, state, &output, format!("{}: render failed: {e:?}", output.name())),
    }
}

/// Gives the cursor `planes` the image's `hotspot` (physical pixels from
/// its top-left), which a virtual GPU passes to the host so its pointer
/// shows the image with the hotspot under it.
fn set_cursor_hotspot(drm: &DrmDevice, planes: &[smithay::reexports::drm::control::plane::Handle], hotspot: Point<i32, Physical>) -> Result<(), String> {
    use smithay::reexports::drm::control::{atomic::AtomicModeReq, property, AtomicCommitFlags};
    if planes.is_empty() {
        return Ok(());
    }
    let mut req = AtomicModeReq::new();
    for plane in planes {
        let props = drm.get_properties(*plane).map_err(|e| e.to_string())?;
        for (handle, _) in &props {
            let Ok(info) = drm.get_property(*handle) else { continue };
            let value = match info.name().to_bytes() {
                b"HOTSPOT_X" => hotspot.x,
                b"HOTSPOT_Y" => hotspot.y,
                _ => continue,
            };
            req.add_property(*plane, *handle, property::Value::SignedRange(value as i64));
        }
    }
    drm.atomic_commit(AtomicCommitFlags::empty(), req).map_err(|e| e.to_string())
}

/// An output got no frame: logged (the first time, then rarely), and when
/// no output has drawn anything since start and this keeps happening, the
/// session stops with the reason instead of staying black (#EDE-40);
/// exponential-session shows it and falls back.
fn frame_failed(u: &mut Udev, state: &mut Ede, output: &Output, error: String) {
    match u.health.failed(&error) {
        super::Failure::Log => tracing::error!("{error}"),
        super::Failure::Quiet => {}
        super::Failure::Fatal => {
            let reason = format!("{} never drew a frame: {error}", output.name());
            tracing::error!("{reason}; stopping");
            state.fatal = Some(reason);
            state.loop_signal.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wants_render;

    /// #EDE-65: an output renders once per refresh while idle, never per
    /// client commit, and never over a pending flip.
    #[test]
    fn an_idle_output_renders_once_per_refresh() {
        assert!(wants_render(false, false), "nothing pending: render");
        assert!(!wants_render(true, false), "a flip waits for its vblank");
        assert!(!wants_render(false, true), "an idle refresh is pending: its timer renders");
    }

    /// #EDE-65 probe: llvmpipe GLES on card0's GBM (vmwgfx without 3D, the
    /// VM's stack) renders partial-damage frames into a rotating swapchain the
    /// way the DrmCompositor does (buffer age per slot); every buffer, read
    /// back through a CPU map of the BO (what scanout reads), must hold the
    /// whole scene. Needs /dev/dri/card0 (any session's ACL, no DRM master).
    #[test]
    #[ignore]
    fn partial_frames_on_card0_hold_the_whole_scene() {
        use smithay::backend::allocator::{dmabuf::AsDmabuf, gbm::{GbmAllocator, GbmBufferFlags, GbmDevice}, Fourcc, Modifier, Swapchain};
        use smithay::backend::drm::DrmDeviceFd;
        use smithay::backend::egl::{EGLContext, EGLDisplay};
        use smithay::backend::renderer::{damage::OutputDamageTracker, element::{solid::SolidColorRenderElement, Id, Kind}, gles::GlesRenderer, utils::CommitCounter, Bind};
        use smithay::utils::{Rectangle, Transform};

        let file = std::fs::OpenOptions::new().read(true).write(true).open("/dev/dri/card0").unwrap();
        let fd = DrmDeviceFd::new(smithay::utils::DeviceFd::from(std::os::fd::OwnedFd::from(file)));
        let gbm = GbmDevice::new(fd).unwrap();
        let display = unsafe { EGLDisplay::new(gbm.clone()) }.unwrap();
        let mut renderer = unsafe { GlesRenderer::new(EGLContext::new(&display).unwrap()) }.unwrap();
        let (w, h) = (256i32, 128i32);
        let allocator = GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
        let mut swapchain = Swapchain::new(allocator, w as u32, h as u32, Fourcc::Argb8888, vec![Modifier::Linear, Modifier::Invalid]);
        let mut tracker = OutputDamageTracker::new((w, h), 1.0, Transform::Normal);
        let square = Id::new();
        let (red, green) = ([1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]);
        let mut held = std::collections::VecDeque::new();
        let mut failures = Vec::new();
        for i in 0..12 {
            let x = 32 * (i % 7);
            let slot = swapchain.acquire().unwrap().expect("a free slot");
            let age = slot.age() as usize;
            let mut dmabuf = slot.export().unwrap();
            {
                let mut fb = renderer.bind(&mut dmabuf).unwrap();
                let elements = [SolidColorRenderElement::new(square.clone(), Rectangle::new((x, 32).into(), (32, 32).into()), CommitCounter::default(), green, Kind::Unspecified)];
                let out = tracker.render_output(&mut renderer, &mut fb, age, &elements, red).unwrap();
                let _ = out.sync.wait();
            }
            swapchain.submitted(&slot);
            // What scanout reads: green under the square, red everywhere else.
            let pixels = slot.map(0, 0, w as u32, h as u32, |m| (m.stride(), m.buffer().to_vec())).unwrap();
            let at = |px: i32, py: i32| {
                let o = py as usize * pixels.0 as usize + px as usize * 4;
                [pixels.1[o + 2], pixels.1[o + 1], pixels.1[o]]
            };
            for sx in (0..w).step_by(16) {
                let want = if sx >= x && sx < x + 32 { [0, 255, 0] } else { [255, 0, 0] };
                if at(sx, 40) != want {
                    failures.push(format!("frame {i} (age {age}) x={sx}: {:?}, want {want:?}", at(sx, 40)));
                }
            }
            held.push_back(slot);
            if held.len() > 2 {
                held.pop_front();
            }
        }
        assert!(failures.is_empty(), "stale pixels in scanout buffers:\n{}", failures.join("\n"));
    }
}
