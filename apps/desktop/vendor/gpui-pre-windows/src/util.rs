use std::sync::OnceLock;

use anyhow::Context;
use gpui_util::ResultExt;
use windows::{
    UI::{
        Color,
        ViewManagement::{UIColorType, UISettings},
    },
    Win32::{
        Foundation::*, Graphics::Dwm::*, System::LibraryLoader::LoadLibraryA,
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, PCSTR},
};

use crate::*;
use gpui::*;

/// Exponential patch (#ERR-3): whether `hwnd` still names a window. A native
/// close (the caption's X, Alt+F4) destroys the window before gpui drops it;
/// teardown and queries asked of it after that only fail.
pub(crate) fn is_live_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

pub(crate) trait HiLoWord {
    fn hiword(&self) -> u16;
    fn loword(&self) -> u16;
    fn signed_hiword(&self) -> i16;
    fn signed_loword(&self) -> i16;
}

impl HiLoWord for WPARAM {
    fn hiword(&self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }

    fn loword(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    fn signed_hiword(&self) -> i16 {
        ((self.0 >> 16) & 0xFFFF) as i16
    }

    fn signed_loword(&self) -> i16 {
        (self.0 & 0xFFFF) as i16
    }
}

impl HiLoWord for LPARAM {
    fn hiword(&self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }

    fn loword(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    fn signed_hiword(&self) -> i16 {
        ((self.0 >> 16) & 0xFFFF) as i16
    }

    fn signed_loword(&self) -> i16 {
        (self.0 & 0xFFFF) as i16
    }
}

pub(crate) unsafe fn get_window_long(hwnd: HWND, nindex: WINDOW_LONG_PTR_INDEX) -> isize {
    #[cfg(target_pointer_width = "64")]
    unsafe {
        GetWindowLongPtrW(hwnd, nindex)
    }
    #[cfg(target_pointer_width = "32")]
    unsafe {
        GetWindowLongW(hwnd, nindex) as isize
    }
}

pub(crate) unsafe fn set_window_long(
    hwnd: HWND,
    nindex: WINDOW_LONG_PTR_INDEX,
    dwnewlong: isize,
) -> isize {
    #[cfg(target_pointer_width = "64")]
    unsafe {
        SetWindowLongPtrW(hwnd, nindex, dwnewlong)
    }
    #[cfg(target_pointer_width = "32")]
    unsafe {
        SetWindowLongW(hwnd, nindex, dwnewlong as i32) as isize
    }
}

/// Exponential patch (#EXP-1923): what an empty input region asks of a
/// window: no click is its own (each goes to the window under it) and it is
/// never activated.
const CLICK_THROUGH: WINDOW_EX_STYLE = WINDOW_EX_STYLE(WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0 | WS_EX_NOACTIVATE.0);

/// Exponential patch (#EXP-1923): `hwnd` takes no mouse input (`on`), or
/// takes it again. A layered window shows nothing until it has attributes:
/// fully opaque, so it looks the same.
pub(crate) fn set_click_through(hwnd: HWND, on: bool) {
    let bits = CLICK_THROUGH.0 as isize;
    unsafe {
        let style = get_window_long(hwnd, GWL_EXSTYLE);
        let next = if on { style | bits } else { style & !bits };
        if next == style {
            return;
        }
        set_window_long(hwnd, GWL_EXSTYLE, next);
        if on {
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA).log_err();
        }
    }
}

pub(crate) fn windows_credentials_target_name(url: &str) -> String {
    format!("zed:url={}", url)
}

pub(crate) fn load_cursor(style: CursorStyle) -> Option<HCURSOR> {
    static ARROW: OnceLock<SafeCursor> = OnceLock::new();
    static IBEAM: OnceLock<SafeCursor> = OnceLock::new();
    static CROSS: OnceLock<SafeCursor> = OnceLock::new();
    static HAND: OnceLock<SafeCursor> = OnceLock::new();
    static SIZEWE: OnceLock<SafeCursor> = OnceLock::new();
    static SIZENS: OnceLock<SafeCursor> = OnceLock::new();
    static SIZENWSE: OnceLock<SafeCursor> = OnceLock::new();
    static SIZENESW: OnceLock<SafeCursor> = OnceLock::new();
    static NO: OnceLock<SafeCursor> = OnceLock::new();
    let (lock, name) = match style {
        CursorStyle::IBeam | CursorStyle::IBeamCursorForVerticalLayout => (&IBEAM, IDC_IBEAM),
        CursorStyle::Crosshair => (&CROSS, IDC_CROSS),
        CursorStyle::PointingHand | CursorStyle::DragLink => (&HAND, IDC_HAND),
        CursorStyle::ResizeLeft
        | CursorStyle::ResizeRight
        | CursorStyle::ResizeLeftRight
        | CursorStyle::ResizeColumn => (&SIZEWE, IDC_SIZEWE),
        CursorStyle::ResizeUp
        | CursorStyle::ResizeDown
        | CursorStyle::ResizeUpDown
        | CursorStyle::ResizeRow => (&SIZENS, IDC_SIZENS),
        CursorStyle::ResizeUpLeftDownRight => (&SIZENWSE, IDC_SIZENWSE),
        CursorStyle::ResizeUpRightDownLeft => (&SIZENESW, IDC_SIZENESW),
        CursorStyle::OperationNotAllowed => (&NO, IDC_NO),
        _ => (&ARROW, IDC_ARROW),
    };
    Some(
        *(*lock.get_or_init(|| {
            HCURSOR(
                unsafe { LoadImageW(None, name, IMAGE_CURSOR, 0, 0, LR_DEFAULTSIZE | LR_SHARED) }
                    .log_err()
                    .unwrap_or_default()
                    .0,
            )
            .into()
        })),
    )
}

/// This function is used to configure the dark mode for the window built-in title bar.
pub(crate) fn configure_dwm_dark_mode(hwnd: HWND, appearance: WindowAppearance) {
    let dark_mode_enabled: BOOL = match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => true.into(),
        WindowAppearance::Light | WindowAppearance::VibrantLight => false.into(),
    };
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode_enabled as *const _ as _,
            std::mem::size_of::<BOOL>() as u32,
        )
        .log_err();
    }
}

#[inline]
pub(crate) fn logical_point(x: f32, y: f32, scale_factor: f32) -> Point<Pixels> {
    Point {
        x: px(x / scale_factor),
        y: px(y / scale_factor),
    }
}

// https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/apply-windows-themes
#[inline]
pub(crate) fn system_appearance() -> Result<WindowAppearance> {
    let ui_settings = UISettings::new()?;
    let foreground_color = ui_settings.GetColorValue(UIColorType::Foreground)?;
    // If the foreground is light, then is_color_light will evaluate to true,
    // meaning Dark mode is enabled.
    if is_color_light(&foreground_color) {
        Ok(WindowAppearance::Dark)
    } else {
        Ok(WindowAppearance::Light)
    }
}

#[inline(always)]
fn is_color_light(color: &Color) -> bool {
    ((5 * color.G as u32) + (2 * color.R as u32) + color.B as u32) > (8 * 128)
}

pub(crate) fn with_dll_library<R, F>(dll_name: PCSTR, f: F) -> Result<R>
where
    F: FnOnce(HMODULE) -> Result<R>,
{
    let library = unsafe {
        LoadLibraryA(dll_name).with_context(|| format!("Loading dll: {}", dll_name.display()))?
    };
    let result = f(library);
    unsafe {
        FreeLibrary(library)
            .with_context(|| format!("Freeing dll: {}", dll_name.display()))
            .log_err();
    }
    result
}

#[cfg(test)]
mod exponential_patch_tests {
    // Not `super::*`: that glob brings `gpui::test`, which shadows `#[test]`.
    use super::{get_window_long, is_live_window, set_click_through};
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GWL_EXSTYLE, GetLayeredWindowAttributes, HWND_MESSAGE,
        LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, WINDOW_EX_STYLE, WINDOW_STYLE, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };
    use windows::core::w;

    /// #ERR-3: a destroyed window reads as gone, so teardown skips it.
    #[test]
    fn a_destroyed_window_is_not_live() {
        unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("probe"),
                WINDOW_STYLE::default(),
                0,
                0,
                10,
                10,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
            .unwrap();
            assert!(is_live_window(hwnd));
            DestroyWindow(hwnd).unwrap();
            assert!(!is_live_window(hwnd));
        }
    }

    /// #EXP-1923: an empty input region makes the window click-through and
    /// never active, fully opaque; a region again takes the clicks back.
    #[test]
    fn an_empty_input_region_is_click_through() {
        let bits = (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0 | WS_EX_NOACTIVATE.0) as isize;
        unsafe {
            // The banner's kind: a top-level PopUp on DirectComposition.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
                w!("STATIC"),
                w!("probe"),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            assert_eq!(get_window_long(hwnd, GWL_EXSTYLE) & bits, 0);
            set_click_through(hwnd, true);
            assert_eq!(get_window_long(hwnd, GWL_EXSTYLE) & bits, bits);
            let (mut key, mut alpha, mut flags) = (COLORREF(0), 0u8, LAYERED_WINDOW_ATTRIBUTES_FLAGS(0));
            GetLayeredWindowAttributes(hwnd, Some(&mut key), Some(&mut alpha), Some(&mut flags)).unwrap();
            assert_eq!((alpha, flags), (255, LWA_ALPHA));
            set_click_through(hwnd, false);
            assert_eq!(get_window_long(hwnd, GWL_EXSTYLE) & bits, 0);
            DestroyWindow(hwnd).unwrap();
        }
    }
}
