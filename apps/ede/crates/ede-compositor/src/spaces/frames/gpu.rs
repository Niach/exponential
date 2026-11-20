//! #EDE-162: the spaces' GPU renderer. A space captured on it is drawn by
//! GLES on a render node instead of by pixman: GPU clients' buffers import
//! there (once their space offers them dmabuf), the composite costs the
//! person's thread little, and the readback is asynchronous (a fence the
//! loop waits on, [`super::queue`]), so the person's thread never waits for
//! the GPU.
//!
//! - **Which GPU** ([`choose`]): `[space_gpu] device` in `compositor.toml`:
//!   `"auto"` (the default), `"software"`, or a render node. `auto` takes,
//!   of the cards with a GPU driver and a render node: on a machine without
//!   a battery with two or more, one the person's displays are not on (an
//!   iGPU beside the discrete card that drives the monitors: the person's
//!   game keeps its GPU to itself); else (one GPU, or a laptop, whose dGPU
//!   stays asleep) the one driving the displays. A machine with one GPU
//!   always uses it. Only drivers the hand proof passed are taken by `auto`
//!   ([`PROVEN`]); a node named in the config is taken whatever its driver
//!   (#EDE-164 proves the others).
//! - **Opened once**, at the compositor's start, on a helper thread (the
//!   driver's EGL init must not stall the person's frames); its own EGL
//!   display (EGL_EXT_platform_device: no GBM, no DRM master, no display)
//!   and its own context at LOW priority where the driver has
//!   EGL_IMG_context_priority. The person's renderer is not touched.
//! - **Accepted only if it works**: a hardware `GL_RENDERER` (not llvmpipe),
//!   fences, dmabuf import with formats to offer. Else, in safe mode, with
//!   `device = "software"` or without a GPU: pixman, as before.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use smithay::{
    backend::{
        allocator::Fourcc,
        drm::DrmNode,
        egl::{context::ContextPriority, fence::EGLFence, EGLContext, EGLDevice, EGLDisplay},
        renderer::{
            damage::OutputDamageTracker,
            gles::{ffi, Capability, GlesError, GlesRenderer, GlesTexture},
            Bind, ImportDma, Offscreen, Renderer,
        },
    },
    utils::{Buffer, Size, Transform},
};

use crate::{desk::DeskId, doctor::DrmCard};

/// Space-renderer drivers the hand proof on mint passed (#EDE-162): the
/// RTX 4090 on nvidia, the Intel iGPU on i915 (xe is its successor). AMD and
/// Mesa as the main GPU are #EDE-164: one line here once proven.
pub const PROVEN: &[&str] = &["nvidia", "i915", "xe"];

/// A space target not captured for this long gives its GPU memory back.
pub const TARGET_IDLE: Duration = Duration::from_secs(30);

/// `[space_gpu] device`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wanted {
    Auto,
    Software,
    Node(PathBuf),
}

impl Wanted {
    pub fn parse(device: &str) -> Result<Wanted, String> {
        match device {
            "auto" => Ok(Wanted::Auto),
            "software" => Ok(Wanted::Software),
            node if node.starts_with("/dev/dri/") => Ok(Wanted::Node(PathBuf::from(node))),
            other => Err(format!("[space_gpu] device is \"auto\", \"software\" or a render node (/dev/dri/renderD128), got {other:?}")),
        }
    }
}

/// The GPU spaces render on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    /// `/dev/dri/renderDN`.
    pub node: PathBuf,
    /// Its card and driver, when the machine's cards name it.
    pub card: Option<String>,
    pub driver: Option<String>,
}

/// What the machine says besides its cards (the rule needs it for laptops).
pub struct Machine<'a> {
    pub cards: &'a [DrmCard],
    /// A battery is present (`/sys/class/power_supply/BAT*`): a laptop,
    /// whose second GPU stays asleep.
    pub battery: bool,
}

/// The GPU spaces render on, or why they render in software.
pub fn choose(wanted: &Wanted, machine: &Machine) -> Result<Picked, String> {
    let picked = |card: &DrmCard| Picked { node: Path::new("/dev/dri").join(card.render_node.as_deref().unwrap_or_default()), card: Some(card.name.clone()), driver: card.driver.clone() };
    match wanted {
        Wanted::Software => Err("[space_gpu] device = \"software\"".into()),
        Wanted::Node(node) => {
            let name = node.file_name().map(|n| n.to_string_lossy().into_owned());
            let card = machine.cards.iter().find(|c| c.render_node.is_some() && c.render_node == name);
            Ok(Picked { node: node.clone(), card: card.map(|c| c.name.clone()), driver: card.and_then(|c| c.driver.clone()) })
        }
        Wanted::Auto => {
            let gpus: Vec<&DrmCard> = machine.cards.iter().filter(|c| c.render_node.is_some() && c.driver.as_deref().is_some_and(crate::doctor::is_gpu_driver)).collect();
            if gpus.is_empty() {
                return Err("no GPU with a render node".into());
            }
            // Preference order: on a machine without a battery with two or
            // more, the ones driving no display; then the displays' card;
            // then the rest.
            let mut order: Vec<&DrmCard> = Vec::new();
            if gpus.len() > 1 && !machine.battery {
                order.extend(gpus.iter().copied().filter(|c| c.connected.is_empty()));
            }
            let owned: Vec<DrmCard> = gpus.iter().map(|c| (*c).clone()).collect();
            if let Some(primary) = crate::doctor::primary_card(&owned) {
                if let Some(c) = gpus.iter().find(|c| c.name == primary.name) {
                    if !order.iter().any(|o| o.name == c.name) {
                        order.push(c);
                    }
                }
            }
            for c in &gpus {
                if !order.iter().any(|o| o.name == c.name) {
                    order.push(c);
                }
            }
            match order.iter().find(|c| c.driver.as_deref().is_some_and(|d| PROVEN.contains(&d))) {
                Some(card) => Ok(picked(card)),
                None => {
                    let drivers: Vec<&str> = order.iter().filter_map(|c| c.driver.as_deref()).collect();
                    Err(format!("{} not proven for agent spaces yet (#EDE-164); `[space_gpu] device = \"/dev/dri/renderD…\"` opts in", drivers.join(", ")))
                }
            }
        }
    }
}

/// The machine as `/sys` describes it.
pub fn this_machine_choice(wanted: &Wanted) -> Result<Picked, String> {
    let sys = Path::new("/sys/class/drm");
    let cards = crate::doctor::read_drm_cards(sys);
    let battery = std::fs::read_dir("/sys/class/power_supply").is_ok_and(|d| d.filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().starts_with("BAT")));
    choose(wanted, &Machine { cards: &cards, battery })
}

/// The EGL display and context of `node`, made off the person's thread:
/// every EGL device naming that render node is tried (glvnd lists one per
/// vendor) until one gives a context.
pub fn open_context(node: &Path) -> Result<EGLContext, String> {
    let want = DrmNode::from_path(node).map_err(|e| format!("{}: {e}", node.display()))?;
    let devices = EGLDevice::enumerate().map_err(|e| format!("no EGL devices: {e}"))?;
    let mut tried = Vec::new();
    for device in devices {
        if device.is_software() || !device.try_get_render_node().ok().flatten().is_some_and(|n| n.dev_id() == want.dev_id()) {
            continue;
        }
        // Safety: the display lives as long as the context holding it; no
        // other code terminates it.
        let display = match unsafe { EGLDisplay::new(device) } {
            Ok(display) => display,
            Err(e) => {
                tried.push(format!("display: {e}"));
                continue;
            }
        };
        match EGLContext::new_with_priority(&display, ContextPriority::Low).or_else(|_| EGLContext::new(&display)) {
            Ok(context) => return Ok(context),
            Err(e) => tried.push(format!("context: {e}")),
        }
    }
    Err(if tried.is_empty() { format!("no EGL device renders on {}", node.display()) } else { format!("{}: {}", node.display(), tried.join("; ")) })
}

/// A space's offscreen target on the GPU.
struct Target {
    texture: GlesTexture,
    tracker: OutputDamageTracker,
    size: (i32, i32),
    /// 0 = never drawn (a full pass), then 1.
    age: usize,
    used: Instant,
    /// A framebuffer on `texture` and the pixel-pack buffer it is read
    /// into, made once per target: `GL_STREAM_READ`, so the driver keeps it
    /// in memory the CPU reads fast (smithay's `copy_framebuffer` makes a new
    /// `STREAM_DRAW` one per call: write-combined memory on a discrete GPU,
    /// read on the person's thread).
    fbo: u32,
    pbo: u32,
}

pub struct SpacesGpu {
    pub renderer: GlesRenderer,
    pub node: PathBuf,
    /// `GL_RENDERER` and the context's priority, for the log and the doctor.
    pub name: String,
    pub priority: Option<ContextPriority>,
    /// The readback can be BGRA (GL_EXT_read_format_bgra): the wire's
    /// bytes as they come.
    bgra: bool,
    targets: HashMap<DeskId, Target>,
}

/// One GPU pass on its way back: the elements (and with them the clients'
/// buffers) are held until the GPU has read them.
pub struct InFlight<E> {
    pub desk: DeskId,
    /// The target's pixel-pack buffer the pixels are on their way into.
    pbo: u32,
    pub fence: EGLFence,
    /// The readback came as RGBA: swap to the wire's BGRA.
    pub swizzle: bool,
    pub size: (i32, i32),
    pub _elements: Vec<E>,
}

impl SpacesGpu {
    /// The renderer on `context`, if it is one spaces may use.
    pub fn new(context: EGLContext, node: PathBuf) -> Result<SpacesGpu, String> {
        let priority = context.get_context_priority();
        // Safety: the context is ours and current only on this thread.
        let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(|e| format!("GLES: {e}"))?;
        let name = renderer
            .with_context(|gl| unsafe {
                let s = gl.GetString(ffi::RENDERER);
                if s.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(s as *const std::ffi::c_char).to_string_lossy().into_owned()
                }
            })
            .map_err(|e| e.to_string())?;
        if crate::doctor::SOFTWARE_RENDERERS.iter().any(|s| name.contains(s)) {
            return Err(format!("{} renders in software ({name})", node.display()));
        }
        if !renderer.capabilities().contains(&Capability::Fencing) {
            return Err(format!("{name}: no fences (GL_OES_EGL_sync)"));
        }
        if renderer.dmabuf_formats().iter().next().is_none() {
            return Err(format!("{name}: imports no dmabuf"));
        }
        let bgra = reads_bgra(&mut renderer);
        Ok(SpacesGpu { renderer, node, name, priority, bgra, targets: HashMap::new() })
    }

    /// Mesa's software EGL device, for the tests (spaces never take it).
    pub fn software_for_tests() -> Result<SpacesGpu, String> {
        let device = EGLDevice::enumerate().map_err(|e| e.to_string())?.find(|d| d.is_software()).ok_or("no software EGL device")?;
        // Safety: as in `open_context`.
        let display = unsafe { EGLDisplay::new(device) }.map_err(|e| e.to_string())?;
        let context = EGLContext::new(&display).map_err(|e| e.to_string())?;
        let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(|e| e.to_string())?;
        let name = renderer
            .with_context(|gl| unsafe { std::ffi::CStr::from_ptr(gl.GetString(ffi::RENDERER) as *const std::ffi::c_char).to_string_lossy().into_owned() })
            .unwrap_or_default();
        let bgra = reads_bgra(&mut renderer);
        Ok(SpacesGpu { renderer, node: PathBuf::from("software"), name, priority: None, bgra, targets: HashMap::new() })
    }

    /// Draws `elements` into space `desk`'s target (only what changed since
    /// its last pass) and starts the readback; nothing here waits for the GPU.
    pub fn submit<E>(&mut self, desk: DeskId, (w, h): (i32, i32), clear: [f32; 4], elements: Vec<E>) -> Result<InFlight<E>, GlesError>
    where
        E: smithay::backend::renderer::element::RenderElement<GlesRenderer>,
    {
        let size: Size<i32, Buffer> = (w, h).into();
        if self.targets.get(&desk).is_none_or(|t| t.size != (w, h)) {
            if let Some(old) = self.targets.remove(&desk) {
                self.free(&[old]);
            }
            let texture = Offscreen::<GlesTexture>::create_buffer(&mut self.renderer, Fourcc::Abgr8888, size)?;
            let tex = texture.tex_id();
            let (fbo, pbo) = self.renderer.with_context(|gl| unsafe {
                let (mut fbo, mut pbo) = (0, 0);
                gl.GenFramebuffers(1, &mut fbo);
                gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
                gl.FramebufferTexture2D(ffi::FRAMEBUFFER, ffi::COLOR_ATTACHMENT0, ffi::TEXTURE_2D, tex, 0);
                gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
                gl.GenBuffers(1, &mut pbo);
                gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, pbo);
                gl.BufferData(ffi::PIXEL_PACK_BUFFER, (w * h * 4) as isize, std::ptr::null(), ffi::STREAM_READ);
                gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, 0);
                (fbo, pbo)
            })?;
            self.targets.insert(desk, Target { texture, tracker: OutputDamageTracker::new((w, h), 1.0, Transform::Normal), size: (w, h), age: 0, used: Instant::now(), fbo, pbo });
        }
        let target = self.targets.get_mut(&desk).expect("just made");
        target.used = Instant::now();
        {
            let mut fb = self.renderer.bind(&mut target.texture)?;
            target.tracker.render_output(&mut self.renderer, &mut fb, target.age, &elements, clear).map_err(|e| match e {
                smithay::backend::renderer::damage::Error::Rendering(e) => e,
                smithay::backend::renderer::damage::Error::OutputNoMode(_) => GlesError::UnknownPixelFormat,
            })?;
        }
        target.age = 1;
        let (fbo, pbo, bgra) = (target.fbo, target.pbo, self.bgra);
        let display = self.renderer.egl_context().display().clone();
        // Into the target's own buffer, asynchronously; the fence must be
        // flushed, or its fd is not exportable and a wait on it may never end.
        let fence = self.renderer.with_context(|gl| unsafe {
            gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
            gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, pbo);
            gl.PixelStorei(ffi::PACK_ALIGNMENT, 4);
            gl.ReadPixels(0, 0, w, h, if bgra { ffi::BGRA_EXT } else { ffi::RGBA }, ffi::UNSIGNED_BYTE, std::ptr::null_mut());
            gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, 0);
            gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
            let fence = EGLFence::create(&display);
            gl.Flush();
            fence
        })?;
        let fence = fence.map_err(|_| GlesError::FramebufferBindingError)?;
        Ok(InFlight { desk, pbo, fence, swizzle: !bgra, size: (w, h), _elements: elements })
    }

    /// The pixels of a pass whose fence signalled, as the wire's `argb8888`
    /// (B, G, R, A bytes).
    pub fn read<E>(&mut self, pass: &InFlight<E>) -> Result<Vec<u8>, GlesError> {
        let len = (pass.size.0 * pass.size.1 * 4) as usize;
        let pbo = pass.pbo;
        let mut out = self.renderer.with_context(|gl| unsafe {
            gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, pbo);
            let ptr = gl.MapBufferRange(ffi::PIXEL_PACK_BUFFER, 0, len as isize, ffi::MAP_READ_BIT);
            let out = (!ptr.is_null()).then(|| std::slice::from_raw_parts(ptr as *const u8, len).to_vec());
            if !ptr.is_null() {
                gl.UnmapBuffer(ffi::PIXEL_PACK_BUFFER);
            }
            gl.BindBuffer(ffi::PIXEL_PACK_BUFFER, 0);
            out
        })?
        .ok_or(GlesError::MappingError)?;
        if pass.swizzle {
            for px in out.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(out)
    }

    /// Frees the targets of spaces gone or not captured for [`TARGET_IDLE`],
    /// and the textures of dead surfaces.
    pub fn sweep(&mut self, live: &[DeskId]) {
        let gone: Vec<DeskId> = self.targets.iter().filter(|(desk, t)| !live.contains(desk) || t.used.elapsed() >= TARGET_IDLE).map(|(desk, _)| *desk).collect();
        let gone: Vec<Target> = gone.into_iter().filter_map(|desk| self.targets.remove(&desk)).collect();
        self.free(&gone);
        let _ = self.renderer.cleanup_texture_cache();
    }

    /// Deletes our framebuffers and pack buffers of `targets` (their
    /// textures go with smithay's cleanup).
    fn free(&mut self, targets: &[Target]) {
        if targets.is_empty() {
            return;
        }
        let names: Vec<(u32, u32)> = targets.iter().map(|t| (t.fbo, t.pbo)).collect();
        let _ = self.renderer.with_context(|gl| unsafe {
            for (fbo, pbo) in &names {
                gl.DeleteFramebuffers(1, fbo);
                gl.DeleteBuffers(1, pbo);
            }
        });
    }

    pub fn has_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    /// Every target draws whole at its next pass (a resume, a GL error: the
    /// GPU may have lost what it held).
    pub fn reset_targets(&mut self) {
        for t in self.targets.values_mut() {
            t.age = 0;
        }
    }
}

/// Whether glReadPixels takes BGRA here (GL_EXT_read_format_bgra).
fn reads_bgra(renderer: &mut GlesRenderer) -> bool {
    renderer
        .with_context(|gl| unsafe {
            let s = gl.GetString(ffi::EXTENSIONS);
            !s.is_null() && std::ffi::CStr::from_ptr(s as *const std::ffi::c_char).to_string_lossy().split_whitespace().any(|e| e == "GL_EXT_read_format_bgra")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, driver: &str, render: Option<&str>, connected: &[&str], boot_vga: bool) -> DrmCard {
        DrmCard { name: name.into(), driver: Some(driver.into()), pci_id: None, boot_vga, render_node: render.map(String::from), connected: connected.iter().map(|s| s.to_string()).collect() }
    }

    fn pick(cards: &[DrmCard], battery: bool) -> Result<PathBuf, String> {
        choose(&Wanted::Auto, &Machine { cards, battery }).map(|p| p.node)
    }

    #[test]
    fn auto_takes_the_gpu_the_displays_are_not_on_when_there_are_two_and_the_one_gpu_when_there_is_one() {
        let nvidia = card("card2", "nvidia", Some("renderD129"), &["DP-2", "HDMI-A-3"], true);
        let intel = card("card1", "i915", Some("renderD128"), &[], false);
        // mint: the 4090 drives the monitors, the iGPU idles: spaces on the iGPU.
        assert_eq!(pick(&[intel.clone(), nvidia.clone()], false), Ok("/dev/dri/renderD128".into()));
        // One GPU (a desktop dGPU without an iGPU, a laptop with one): that
        // one, never software for want of a second.
        assert_eq!(pick(&[nvidia.clone()], false), Ok("/dev/dri/renderD129".into()));
        assert_eq!(pick(&[intel.clone()], true), Ok("/dev/dri/renderD128".into()));
        // The iGPU drives the displays, a dGPU idles beside it (a desktop):
        // the dGPU.
        let panel = card("card1", "i915", Some("renderD128"), &["eDP-1"], true);
        let dgpu = card("card2", "nvidia", Some("renderD129"), &[], false);
        assert_eq!(pick(&[panel.clone(), dgpu.clone()], false), Ok("/dev/dri/renderD129".into()));
        // The same on a laptop: the iGPU that drives the panel; the dGPU sleeps on.
        assert_eq!(pick(&[panel, dgpu], true), Ok("/dev/dri/renderD128".into()));
        // A firmware framebuffer and a VM's GPU without a render node are no GPU.
        let simple = card("card0", "simpledrm", None, &["Virtual-1"], true);
        assert!(pick(&[simple], false).is_err());
    }

    #[test]
    fn auto_takes_only_proven_drivers_and_a_named_node_whatever_its_driver() {
        let amd = card("card0", "amdgpu", Some("renderD128"), &["DP-1"], true);
        let why = pick(&[amd.clone()], false).unwrap_err();
        assert!(why.contains("amdgpu not proven") && why.contains("#EDE-164"), "{why}");
        // An idle AMD iGPU beside NVIDIA's displays: it would come first, it
        // is not proven, so the proven displays' GPU is taken.
        let amd_idle = card("card0", "amdgpu", Some("renderD128"), &[], false);
        let nvidia = card("card1", "nvidia", Some("renderD129"), &["DP-1"], true);
        assert_eq!(pick(&[amd_idle, nvidia], false), Ok("/dev/dri/renderD129".into()));
        let named = choose(&Wanted::Node("/dev/dri/renderD128".into()), &Machine { cards: &[amd], battery: false }).unwrap();
        assert_eq!((named.node, named.driver.as_deref()), (PathBuf::from("/dev/dri/renderD128"), Some("amdgpu")));
        assert!(choose(&Wanted::Software, &Machine { cards: &[], battery: false }).is_err());
    }

    #[test]
    fn the_config_names_auto_software_or_a_render_node() {
        assert_eq!(Wanted::parse("auto"), Ok(Wanted::Auto));
        assert_eq!(Wanted::parse("software"), Ok(Wanted::Software));
        assert_eq!(Wanted::parse("/dev/dri/renderD128"), Ok(Wanted::Node("/dev/dri/renderD128".into())));
        assert!(Wanted::parse("nvidia").is_err());
    }
}
