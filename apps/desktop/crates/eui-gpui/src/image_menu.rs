//! #EXP-1947 (Exponential's `fixtures/image-actions.json`, the gpui part;
//! the web renderers' `image-actions.ts`, Compose's `ImageMenu`): a content
//! picture's menu. The renderer draws no menu of its own: once a content
//! picture's bytes RENDERED (`Image` + Carousel / Mosaic slides,
//! `ZoomableImage`, a Markdown block image) it hands the picture to
//! [`HostPlugin::image_menu`], which wraps it in the host's right-click menu
//! (a press on it is the image menu's, never merged with a menu around it).
//! A picture still loading, failed or refused is never handed over: a press
//! on it falls through to the element around it. Avatars, video posters and
//! the choice tiles are not content pictures.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, SharedString, Window};

use crate::host::{HostPlugin, ImageTarget};

/// `picture` (showing `src`) answering the host's image menu once its bytes
/// rendered; built at layout, where the decode state is known.
pub fn with_image_menu(host: Rc<dyn HostPlugin>, id: impl Into<SharedString>, src: &str, alt: &str, picture: AnyElement) -> AnyElement {
    let (id, src, alt) = (id.into(), src.to_string(), alt.to_string());
    WithWindow::new(move |window, cx| {
        if !host.has_image_menu(cx) || !image_ready(host.as_ref(), &src, window, cx) {
            return picture;
        }
        let target = ImageTarget::new(host.as_ref(), id, &src, &alt);
        host.image_menu(target, picture, window, cx)
    })
    .into_any_element()
}

/// The picture of `src` decoded (not loading, failed or refused).
pub fn image_ready(host: &dyn HostPlugin, src: &str, window: &mut Window, cx: &mut App) -> bool {
    crate::media::image_source(host, src).is_some_and(|source| matches!(crate::paint::natives::render_image(&source, window, cx), Some(Ok(_))))
}

/// An element built at layout with the window at hand (the decode state of
/// a picture is only known there).
pub struct WithWindow {
    build: Option<Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>>,
    child: Option<AnyElement>,
}

impl WithWindow {
    pub fn new(build: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        Self { build: Some(Box::new(build)), child: None }
    }
}

impl IntoElement for WithWindow {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for WithWindow {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, window: &mut Window, cx: &mut App) -> (gpui::LayoutId, ()) {
        let mut child = match self.build.take() {
            Some(build) => build(window, cx),
            None => gpui::Empty.into_any_element(),
        };
        let layout = child.request_layout(window, cx);
        self.child = Some(child);
        (layout, ())
    }

    fn prepaint(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, _: gpui::Bounds<gpui::Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
    }

    fn paint(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, _: gpui::Bounds<gpui::Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}
