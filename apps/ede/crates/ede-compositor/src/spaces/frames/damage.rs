//! #EDE-162: a space's damage check without a renderer. The tick only asks
//! whether the scene changed. Building render elements for that would
//! import every buffer (`S/backend/renderer/element/surface.rs` calls
//! `import_surface`): a GPU client's dmabuf, which pixman cannot read, so
//! its window would never move the serial; or, on a GPU renderer, a texture
//! upload per tick on the thread that serves the person's frames. These
//! elements carry what the damage tracker reads (place, size, commits,
//! damage, opaque regions) straight from each surface's renderer state:
//! the numbers smithay's `WaylandSurfaceRenderElement` gives it, without a
//! texture.

use smithay::{
    backend::renderer::{
        element::{solid::SolidColorRenderElement, Element, Id, Kind},
        utils::{CommitCounter, DamageSet, DamageSnapshot, OpaqueRegions, RendererSurfaceStateUserData, SurfaceView},
    },
    desktop::{PopupManager, Window},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform},
    wayland::{
        compositor::{with_surface_tree_downward, SurfaceData, TraversalAction},
        seat::WaylandFocus,
    },
};

/// One element of a space's scene as the damage tracker sees it.
pub enum SpaceDamage {
    Surface(SurfaceDamage),
    /// A protected window, solid black (its id is the window's own,
    /// [`protected_id`]).
    Solid(SolidColorRenderElement),
}

/// A surface with a buffer: what `WaylandSurfaceRenderElement` would be,
/// without its texture.
pub struct SurfaceDamage {
    id: Id,
    location: Point<f64, Physical>,
    alpha: f32,
    view: SurfaceView,
    buffer_scale: i32,
    buffer_transform: Transform,
    buffer_dimensions: Size<i32, BufferCoords>,
    damage: DamageSnapshot<i32, BufferCoords>,
    opaque_regions: OpaqueRegions<i32, Logical>,
}

/// The id of a protected window's black element: the same one on every
/// call, so a still protected window is no damage (a new `Id` per call
/// moved its space's serial on every tick).
pub fn protected_id(window: &Window) -> Id {
    struct ProtectedId(Id);
    window.user_data().insert_if_missing_threadsafe(|| ProtectedId(Id::new()));
    window.user_data().get::<ProtectedId>().map(|p| p.0.clone()).unwrap_or_else(Id::new)
}

/// `window` at `location` (in the space's output's pixels, scale 1), front
/// to back: its popups, then its surface tree, as `AsRenderElements for
/// Window` lists them.
pub fn window_damage(window: &Window, location: Point<i32, Physical>, out: &mut Vec<SpaceDamage>) {
    let Some(surface) = window.wl_surface().map(|s| s.into_owned()) else { return };
    // An X11 window's opacity, as smithay's X11 elements carry it (an
    // element under 1 has no opaque region). Wayland clients cannot set
    // one: no alpha-modifier global is offered.
    #[cfg(feature = "xwayland")]
    let alpha = window.x11_surface().and_then(|x| x.opacity()).map_or(1.0, |o| o as f32 / u32::MAX as f32);
    #[cfg(not(feature = "xwayland"))]
    let alpha = 1.0;
    if window.is_wayland() {
        for (popup, offset) in PopupManager::popups_for_surface(&surface) {
            let offset = (window.geometry().loc + offset - popup.geometry().loc).to_physical(1);
            surface_tree(popup.wl_surface(), location + offset, 1.0, out);
        }
    }
    surface_tree(&surface, location, alpha, out);
}

/// `render_elements_from_surface_tree` at scale 1, without importing.
fn surface_tree(surface: &WlSurface, location: Point<i32, Physical>, alpha: f32, out: &mut Vec<SpaceDamage>) {
    let view_of = |states: &SurfaceData| states.data_map.get::<RendererSurfaceStateUserData>().and_then(|d| d.lock().unwrap().view());
    with_surface_tree_downward(
        surface,
        location.to_f64(),
        |_, states, location| match view_of(states) {
            Some(view) => TraversalAction::DoChildren(*location + view.offset.to_f64().to_physical(1.0)),
            None => TraversalAction::SkipChildren,
        },
        |surface, states, location| {
            let Some(view) = view_of(states) else { return };
            if let Some(element) = SurfaceDamage::new(surface, states, *location + view.offset.to_f64().to_physical(1.0), alpha) {
                out.push(SpaceDamage::Surface(element));
            }
        },
        |_, _, _| true,
    );
}

impl SurfaceDamage {
    /// `None` = no buffer (not mapped), as `from_state` answers.
    fn new(surface: &WlSurface, states: &SurfaceData, location: Point<f64, Physical>, alpha: f32) -> Option<Self> {
        let data = states.data_map.get::<RendererSurfaceStateUserData>()?.lock().unwrap();
        data.buffer()?;
        let (buffer_scale, buffer_transform) = (data.buffer_scale(), data.buffer_transform());
        Some(Self {
            id: Id::from_wayland_resource(surface),
            location,
            alpha,
            view: data.view()?,
            buffer_scale,
            buffer_transform,
            buffer_dimensions: data.buffer_size()?.to_buffer(buffer_scale, buffer_transform),
            damage: data.damage(),
            opaque_regions: data.opaque_regions().map(OpaqueRegions::from_slice).unwrap_or_default(),
        })
    }

    fn size(&self, scale: Scale<f64>) -> Size<i32, Physical> {
        ((self.view.dst.to_f64().to_physical(scale).to_point() + self.location).to_i32_round() - self.location.to_i32_round()).to_size()
    }

    fn buffer_size(&self) -> Size<i32, Logical> {
        self.buffer_dimensions.to_logical(self.buffer_scale, self.buffer_transform)
    }
}

// The same answers as `WaylandSurfaceRenderElement`'s `Element`
// (smithay `backend/renderer/element/surface.rs`).
impl Element for SurfaceDamage {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.damage.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(self.location.to_i32_round(), self.size(scale))
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.view.src.to_buffer(self.buffer_scale as f64, self.buffer_transform, &self.buffer_size().to_f64())
    }

    fn transform(&self) -> Transform {
        self.buffer_transform
    }

    fn damage_since(&self, scale: Scale<f64>, commit: Option<CommitCounter>) -> DamageSet<i32, Physical> {
        let dst_size = self.size(scale);
        self.damage
            .damage_since(commit)
            .unwrap_or_else(|| DamageSet::from_slice(&[Rectangle::from_size(self.buffer_dimensions)]))
            .iter()
            .filter_map(|rect| {
                rect.to_f64()
                    .to_logical(self.buffer_scale as f64, self.buffer_transform, &self.buffer_dimensions.to_f64())
                    .intersection(self.view.src)
                    .map(|rect| rect_to_global(&self.view, rect).to_i32_up::<i32>())
                    .map(|rect| {
                        let surface_scale = dst_size.to_f64() / self.view.dst.to_f64().to_physical(scale);
                        rect.to_physical_precise_up(surface_scale * scale)
                    })
            })
            .collect::<DamageSet<_, _>>()
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        if self.alpha < 1.0 {
            return OpaqueRegions::default();
        }
        self.opaque_regions
            .iter()
            .map(|r| {
                let loc = r.loc.to_physical_precise_round(scale);
                let size = ((r.size.to_f64().to_physical(scale).to_point() + self.location).to_i32_round() - self.location.to_i32_round()).to_size();
                Rectangle::new(loc, size)
            })
            .collect::<OpaqueRegions<_, _>>()
    }

    fn alpha(&self) -> f32 {
        self.alpha
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }
}

/// `SurfaceView::rect_to_global` (crate-private in smithay): a rect in the
/// surface's source into its destination.
fn rect_to_global(view: &SurfaceView, rect: Rectangle<f64, Logical>) -> Rectangle<f64, Logical> {
    let scale = Scale::from((view.dst.w as f64 / view.src.size.w, view.dst.h as f64 / view.src.size.h));
    let mut rect = rect;
    rect.loc -= view.src.loc;
    rect.upscale(scale)
}

impl Element for SpaceDamage {
    fn id(&self) -> &Id {
        match self {
            Self::Surface(e) => e.id(),
            Self::Solid(e) => e.id(),
        }
    }

    fn current_commit(&self) -> CommitCounter {
        match self {
            Self::Surface(e) => e.current_commit(),
            Self::Solid(e) => e.current_commit(),
        }
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        match self {
            Self::Surface(e) => e.location(scale),
            Self::Solid(e) => e.location(scale),
        }
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        match self {
            Self::Surface(e) => e.src(),
            Self::Solid(e) => e.src(),
        }
    }

    fn transform(&self) -> Transform {
        match self {
            Self::Surface(e) => e.transform(),
            Self::Solid(e) => e.transform(),
        }
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match self {
            Self::Surface(e) => e.geometry(scale),
            Self::Solid(e) => e.geometry(scale),
        }
    }

    fn damage_since(&self, scale: Scale<f64>, commit: Option<CommitCounter>) -> DamageSet<i32, Physical> {
        match self {
            Self::Surface(e) => e.damage_since(scale, commit),
            Self::Solid(e) => e.damage_since(scale, commit),
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        match self {
            Self::Surface(e) => e.opaque_regions(scale),
            Self::Solid(e) => e.opaque_regions(scale),
        }
    }

    fn alpha(&self) -> f32 {
        match self {
            Self::Surface(e) => e.alpha(),
            Self::Solid(e) => e.alpha(),
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Self::Surface(e) => e.kind(),
            Self::Solid(e) => e.kind(),
        }
    }
}
