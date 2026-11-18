// Exponential patch (#EXP-1912): a window's input shape (the X Shape
// extension, kind Input): the rectangles of the window that take pointer
// events. An empty shape makes the window click-through: every pointer
// event goes to the window under it. Upstream leaves
// `PlatformWindow::set_input_region` a no-op on X11, so the computer-use
// banner (an override-redirect window above everything) took the clicks
// meant for the app under it.

use gpui::{Bounds, Pixels};
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{self, ConnectionExt as _},
        xproto,
    },
};

/// `region` (logical pixels) as the shape's rectangles in device pixels.
pub(crate) fn rectangles(region: &[Bounds<Pixels>], scale_factor: f32) -> Vec<xproto::Rectangle> {
    let device = |logical: Pixels| (f32::from(logical) * scale_factor).round();
    region
        .iter()
        .map(|rect| xproto::Rectangle {
            x: device(rect.origin.x) as i16,
            y: device(rect.origin.y) as i16,
            width: device(rect.size.width).max(0.) as u16,
            height: device(rect.size.height).max(0.) as u16,
        })
        .collect()
}

/// Sets the input shape of `window`: `None` = the whole window again (the
/// default), `Some(&[])` = no input at all.
pub(crate) fn set(
    xcb: &impl Connection,
    window: xproto::Window,
    rectangles: Option<&[xproto::Rectangle]>,
) -> anyhow::Result<()> {
    match rectangles {
        None => xcb.shape_mask(shape::SO::SET, shape::SK::INPUT, window, 0, 0, x11rb::NONE)?,
        Some(rectangles) => xcb.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            xproto::ClipOrdering::UNSORTED,
            window,
            0,
            0,
            rectangles,
        )?,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::{Child, Command, Stdio};

    use gpui::{Bounds, point, px, size};
    use x11rb::{
        connection::Connection,
        protocol::{
            shape::{self, ConnectionExt as _},
            xproto::{self, ConnectionExt as _},
        },
        rust_connection::RustConnection,
    };

    use super::{rectangles, set};

    /// x11rb's `Rectangle` has no `PartialEq` without its extra traits.
    fn boxes(rectangles: &[xproto::Rectangle]) -> Vec<(i16, i16, u16, u16)> {
        rectangles.iter().map(|r| (r.x, r.y, r.width, r.height)).collect()
    }

    #[test]
    fn input_shape_rectangles_are_device_pixels() {
        let region = [Bounds {
            origin: point(px(10.), px(20.5)),
            size: size(px(100.), px(40.)),
        }];
        assert_eq!(boxes(&rectangles(&region, 2.)), [(20, 41, 200, 80)]);
        assert!(rectangles(&[], 2.).is_empty());
    }

    /// A private X server (never the person's display); `None` = no Xvfb here.
    struct Server {
        child: Child,
        display: String,
    }

    impl Server {
        fn start() -> Option<Self> {
            for number in 4200 + std::process::id() % 500.. {
                let display = format!(":{number}");
                let Ok(mut child) = Command::new("Xvfb")
                    .args([display.as_str(), "-screen", "0", "640x480x24", "-nolisten", "tcp"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                else {
                    return None;
                };
                for _ in 0..200 {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    if child.try_wait().ok().flatten().is_some() {
                        // The display number was taken: the next one.
                        break;
                    }
                    if x11rb::connect(Some(&display)).is_ok() {
                        return Some(Server { child, display });
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                if number > 4200 + 520 {
                    return None;
                }
            }
            None
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn input_shape(xcb: &RustConnection, window: xproto::Window) -> Vec<(i16, i16, u16, u16)> {
        boxes(&xcb.shape_get_rectangles(window, shape::SK::INPUT).unwrap().reply().unwrap().rectangles)
    }

    /// The window the pointer is in at `(x, y)` of the root.
    fn window_at(xcb: &RustConnection, root: xproto::Window, x: i16, y: i16) -> xproto::Window {
        xcb.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, x, y).unwrap();
        xcb.query_pointer(root).unwrap().reply().unwrap().child
    }

    /// An override-redirect window above another (the banner over an app):
    /// with an empty input shape the pointer is in the window under it.
    #[test]
    fn input_shape_empty_lets_the_pointer_through() {
        let Some(server) = Server::start() else {
            eprintln!("skipped: no Xvfb binary");
            return;
        };
        let (xcb, screen) = x11rb::connect(Some(&server.display)).unwrap();
        let root = xcb.setup().roots[screen].root;
        let open = |x: i16, y: i16, width: u16, height: u16| {
            let window = xcb.generate_id().unwrap();
            let aux = xproto::CreateWindowAux::new().override_redirect(1);
            xcb.create_window(x11rb::COPY_DEPTH_FROM_PARENT, window, root, x, y, width, height, 0, xproto::WindowClass::INPUT_OUTPUT, 0, &aux)
                .unwrap();
            xcb.map_window(window).unwrap();
            window
        };
        let app = open(0, 0, 400, 300);
        let banner = open(100, 200, 200, 50);
        xcb.flush().unwrap();

        let whole = [(0, 0, 200, 50)];
        assert_eq!(input_shape(&xcb, banner), whole);
        assert_eq!(window_at(&xcb, root, 150, 220), banner, "the default: the banner takes the pointer");

        set(&xcb, banner, Some(&[])).unwrap();
        assert!(input_shape(&xcb, banner).is_empty());
        assert_eq!(window_at(&xcb, root, 150, 220), app, "the banner still took the pointer");

        // A part of the window, then the whole window again.
        set(&xcb, banner, Some(&[xproto::Rectangle { x: 0, y: 0, width: 20, height: 50 }])).unwrap();
        assert_eq!(window_at(&xcb, root, 110, 220), banner);
        assert_eq!(window_at(&xcb, root, 150, 220), app);
        set(&xcb, banner, None).unwrap();
        assert_eq!(input_shape(&xcb, banner), whole);
        assert_eq!(window_at(&xcb, root, 150, 220), banner);
    }
}
