//! Hand-declared C-ABI bindings for the embeddable libghostty
//! (vendor/ghostty-install/include/ghostty.h, built from the douglas/ghostty
//! fork @ c5028f9 with `-Dapp-runtime=none`). Same philosophy as gtk.zig: we
//! declare the C ABI by hand rather than @cImport.
//!
//! IMPORTANT: structs passed BY VALUE across the FFI (runtime_config_s,
//! surface_config_s, input_key_s, and the action/target structs handed to the
//! action callback) must match the header's layout exactly — a size/alignment
//! mismatch corrupts the stack. These mirror the c5028f9 header verbatim.
//!
//! Linked in build.zig:linkLibghostty (libghostty.so + libglad.so + libGL).

const std = @import("std");

pub const App = ?*anyopaque; // ghostty_app_t
pub const Config = ?*anyopaque; // ghostty_config_t
pub const Surface = ?*anyopaque; // ghostty_surface_t

pub const SUCCESS: c_int = 0;

// --- platform tag (ghostty_platform_e) ---
pub const PLATFORM_INVALID: c_int = 0;
pub const PLATFORM_MACOS: c_int = 1;
pub const PLATFORM_IOS: c_int = 2;
pub const PLATFORM_LINUX: c_int = 3;

// --- surface context (ghostty_surface_context_e) ---
pub const SURFACE_CONTEXT_WINDOW: c_int = 0;
pub const SURFACE_CONTEXT_TAB: c_int = 1;
pub const SURFACE_CONTEXT_SPLIT: c_int = 2;

// --- surface IO mode (ghostty_surface_io_mode_e) ---
pub const SURFACE_IO_EXEC: c_int = 0;
pub const SURFACE_IO_MANUAL: c_int = 1;

// --- input action (ghostty_input_action_e) ---
pub const ACTION_RELEASE: c_int = 0;
pub const ACTION_PRESS: c_int = 1;
pub const ACTION_REPEAT: c_int = 2;

// --- input mods (ghostty_input_mods_e) ---
pub const MODS_NONE: c_int = 0;
pub const MODS_SHIFT: c_int = 1 << 0;
pub const MODS_CTRL: c_int = 1 << 1;
pub const MODS_ALT: c_int = 1 << 2;
pub const MODS_SUPER: c_int = 1 << 3;
pub const MODS_CAPS: c_int = 1 << 4;

// --- mouse state (ghostty_input_mouse_state_e) ---
pub const MOUSE_RELEASE: c_int = 0;
pub const MOUSE_PRESS: c_int = 1;

// --- mouse button (ghostty_input_mouse_button_e) ---
pub const MOUSE_UNKNOWN: c_int = 0;
pub const MOUSE_LEFT: c_int = 1;
pub const MOUSE_RIGHT: c_int = 2;
pub const MOUSE_MIDDLE: c_int = 3;

// --- action tag (ghostty_action_tag_e) — only the ones we inspect. These
// indices match the header's enum order at c5028f9; if the pinned commit
// changes, re-derive them. Used to detect child/command exit for agent runs. ---
pub const ACTION_RENDER: c_int = 27;
pub const ACTION_SHOW_CHILD_EXITED: c_int = 54;
pub const ACTION_COMMAND_FINISHED: c_int = 57;

// --- structs ---

pub const env_var_s = extern struct {
    key: ?[*:0]const u8,
    value: ?[*:0]const u8,
};

const platform_macos_s = extern struct { nsview: ?*anyopaque };
const platform_ios_s = extern struct { uiview: ?*anyopaque };
const platform_linux_s = extern struct { gl_area: ?*anyopaque };
const platform_u = extern union {
    macos: platform_macos_s,
    ios: platform_ios_s,
    linux: platform_linux_s,
};

pub const io_write_cb = ?*const fn (?*anyopaque, [*c]const u8, usize) callconv(.c) void;

pub const surface_config_s = extern struct {
    platform_tag: c_int,
    platform: platform_u,
    userdata: ?*anyopaque,
    scale_factor: f64,
    font_size: f32,
    working_directory: ?[*:0]const u8,
    command: ?[*:0]const u8,
    env_vars: ?[*]env_var_s,
    env_var_count: usize,
    initial_input: ?[*:0]const u8,
    wait_after_command: bool,
    context: c_int,
    io_mode: c_int,
    io_write_cb: io_write_cb,
    io_write_userdata: ?*anyopaque,
};

pub const surface_size_s = extern struct {
    columns: u16,
    rows: u16,
    width_px: u32,
    height_px: u32,
    cell_width_px: u32,
    cell_height_px: u32,
};

// ghostty_input_key_s — passed by value to ghostty_surface_key.
pub const input_key_s = extern struct {
    action: c_int,
    mods: c_int,
    consumed_mods: c_int,
    keycode: u32,
    text: ?[*:0]const u8,
    unshifted_codepoint: u32,
    composing: bool,
};

// ghostty_target_s (16 bytes): tag + union{surface}.
const target_u = extern union { surface: Surface };
pub const target_s = extern struct {
    tag: c_int,
    target: target_u,
};

// Action payloads we read (Stage 2 result capture).
pub const childexited_s = extern struct { exit_code: u32, timetime_ms: u64 };
pub const command_finished_s = extern struct { exit_code: i16, duration: u64 };

// ghostty_action_u — sized to the header's largest member (24 bytes, e.g.
// scrollbar_s = 3×u64). `_max` pins the size/alignment so the by-value ABI
// matches even though we only name the variants we inspect.
pub const action_u = extern union {
    child_exited: childexited_s,
    command_finished: command_finished_s,
    _max: [3]u64,
};
pub const action_s = extern struct {
    tag: c_int,
    action: action_u,
};

// Runtime callbacks (ghostty_runtime_config_s).
pub const wakeup_cb = ?*const fn (?*anyopaque) callconv(.c) void;
pub const action_cb = ?*const fn (App, target_s, action_s) callconv(.c) bool;
pub const read_clipboard_cb = ?*const fn (?*anyopaque, c_int, ?*anyopaque) callconv(.c) void;
pub const confirm_read_clipboard_cb = ?*const fn (?*anyopaque, [*c]const u8, ?*anyopaque, c_int) callconv(.c) void;
pub const write_clipboard_cb = ?*const fn (?*anyopaque, c_int, ?*const anyopaque, usize, bool) callconv(.c) void;
pub const close_surface_cb = ?*const fn (?*anyopaque, bool) callconv(.c) void;

pub const runtime_config_s = extern struct {
    userdata: ?*anyopaque,
    supports_selection_clipboard: bool,
    wakeup_cb: wakeup_cb,
    action_cb: action_cb,
    read_clipboard_cb: read_clipboard_cb,
    confirm_read_clipboard_cb: confirm_read_clipboard_cb,
    write_clipboard_cb: write_clipboard_cb,
    close_surface_cb: close_surface_cb,
};

// --- published API (subset we use) ---
pub extern fn ghostty_init(argc: usize, argv: ?[*]?[*:0]u8) c_int;

pub extern fn ghostty_config_new() Config;
pub extern fn ghostty_config_free(Config) void;
pub extern fn ghostty_config_load_default_files(Config) void;
pub extern fn ghostty_config_load_recursive_files(Config) void;
pub extern fn ghostty_config_finalize(Config) void;

pub extern fn ghostty_app_new(*const runtime_config_s, Config) App;
pub extern fn ghostty_app_free(App) void;
pub extern fn ghostty_app_tick(App) void;
pub extern fn ghostty_app_set_focus(App, bool) void;

pub extern fn ghostty_surface_new(App, *const surface_config_s) Surface;
pub extern fn ghostty_surface_free(Surface) void;
pub extern fn ghostty_surface_userdata(Surface) ?*anyopaque;
pub extern fn ghostty_surface_process_exited(Surface) bool;
pub extern fn ghostty_surface_refresh(Surface) void;
pub extern fn ghostty_surface_draw(Surface) void;
pub extern fn ghostty_surface_display_realized(Surface) void;
pub extern fn ghostty_surface_set_content_scale(Surface, f64, f64) void;
pub extern fn ghostty_surface_set_focus(Surface, bool) void;
pub extern fn ghostty_surface_set_size(Surface, u32, u32) void;
pub extern fn ghostty_surface_size(Surface) surface_size_s;
pub extern fn ghostty_surface_key(Surface, input_key_s) bool;
pub extern fn ghostty_surface_text(Surface, [*]const u8, usize) void;
pub extern fn ghostty_surface_mouse_button(Surface, c_int, c_int, c_int) bool;
pub extern fn ghostty_surface_mouse_pos(Surface, f64, f64, c_int) void;
pub extern fn ghostty_surface_mouse_scroll(Surface, f64, f64, c_int) void;
pub extern fn ghostty_surface_request_close(Surface) void;

// --- raw GL (GtkGLArea doesn't set glViewport before "render"; ghostty's
// renderer reads GL_VIEWPORT). Resolved from libGL in build.zig. ---
pub const GL_COLOR_BUFFER_BIT: c_uint = 0x00004000;
pub extern fn glViewport(x: c_int, y: c_int, width: c_int, height: c_int) void;
pub extern fn glClearColor(r: f32, g: f32, b: f32, a: f32) void;
pub extern fn glClear(mask: c_uint) void;
