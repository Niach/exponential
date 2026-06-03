//! Hand-declared GTK4 + libadwaita + GIO bindings.
//!
//! We deliberately avoid `@cImport(gtk/gtk.h)`: Zig 0.16's translate-c can't
//! parse GLib's `_Pragma`-based macros (G_GNUC_BEGIN_IGNORE_DEPRECATIONS, the
//! "include only <gdk/gdk.h>" guards), producing thousands of errors. Declaring
//! the C ABI by hand is reliable. Every GObject instance is just a pointer, so
//! we model them all as `?*anyopaque` and pass widgets around freely — the C
//! side does the runtime type checks. Functions are resolved at link time from
//! gtk4 / libadwaita-1 / gio-2.0 (wired in build.zig:linkGui).

/// Opaque GObject pointer (GtkWidget*, GtkWindow*, AdwApplication*, …).
pub const Object = ?*anyopaque;
pub const gpointer = ?*anyopaque;

/// GCallback — a type-erased C function pointer; cast concrete handlers with
/// @ptrCast when connecting signals.
pub const GCallback = ?*const fn () callconv(.c) void;

/// GApplicationFlags.G_APPLICATION_DEFAULT_FLAGS (== 0).
pub const APP_DEFAULT_FLAGS: c_uint = 0;
/// GApplicationFlags.G_APPLICATION_HANDLES_COMMAND_LINE — forwards a second
/// instance's argv to the primary (used to capture the exp:// OAuth redirect).
pub const APP_HANDLES_COMMAND_LINE: c_uint = 8;

/// GtkOrientation
pub const ORIENTATION_HORIZONTAL: c_int = 0;
pub const ORIENTATION_VERTICAL: c_int = 1;
/// GtkAlign
pub const ALIGN_FILL: c_int = 0;
pub const ALIGN_START: c_int = 1;
pub const ALIGN_END: c_int = 2;
pub const ALIGN_CENTER: c_int = 3;

// --- GObject / GLib ---
pub extern fn g_object_unref(object: Object) void;
pub extern fn g_signal_connect_data(
    instance: Object,
    detailed_signal: [*:0]const u8,
    c_handler: GCallback,
    data: gpointer,
    destroy_data: gpointer,
    connect_flags: c_uint,
) c_ulong;
pub extern fn g_application_run(app: Object, argc: c_int, argv: ?[*]const [*:0]const u8) c_int;
pub extern fn g_application_quit(app: Object) void;
pub extern fn g_idle_add(function: ?*const fn (gpointer) callconv(.c) c_int, data: gpointer) c_uint;
pub extern fn g_strfreev(str_array: ?[*]?[*:0]u8) void;

// GApplicationCommandLine — read a (possibly remote) instance's argv. This is
// how the exp:// OAuth redirect reaches the primary instance.
pub extern fn g_application_command_line_get_arguments(cmdline: Object, argc: *c_int) ?[*]?[*:0]u8;

// --- GIO (open the OAuth URL in the default browser; register scheme handler) ---
pub extern fn g_app_info_launch_default_for_uri(uri: [*:0]const u8, context: gpointer, @"error": gpointer) c_int;
pub extern fn g_spawn_command_line_async(command_line: [*:0]const u8, @"error": gpointer) c_int;

// --- Adwaita ---
pub extern fn adw_application_new(app_id: [*:0]const u8, flags: c_uint) Object;
pub extern fn adw_application_window_new(app: Object) Object;
pub extern fn adw_application_window_set_content(self: Object, content: Object) void;
pub extern fn adw_toolbar_view_new() Object;
pub extern fn adw_toolbar_view_add_top_bar(self: Object, widget: Object) void;
pub extern fn adw_toolbar_view_set_content(self: Object, content: Object) void;
pub extern fn adw_header_bar_new() Object;
pub extern fn adw_window_title_new(title: [*:0]const u8, subtitle: [*:0]const u8) Object;
pub extern fn adw_header_bar_set_title_widget(self: Object, title_widget: Object) void;
pub extern fn adw_header_bar_pack_end(self: Object, child: Object) void;
pub extern fn adw_header_bar_set_show_start_title_buttons(self: Object, setting: c_int) void;
pub extern fn adw_header_bar_set_show_end_title_buttons(self: Object, setting: c_int) void;
// AdwStyleManager — the web app is hard dark-only, so we force the dark scheme
// rather than following the desktop's light/dark preference.
pub extern fn adw_style_manager_get_default() Object;
pub extern fn adw_style_manager_set_color_scheme(self: Object, color_scheme: c_int) void;
pub const ADW_COLOR_SCHEME_FORCE_DARK: c_int = 4;
// AdwNavigationView — list → detail subpage navigation with an auto back button
// (the web-like "navigate to a subpage" model, replacing the 3rd detail pane).
pub extern fn adw_navigation_view_new() Object;
pub extern fn adw_navigation_view_push(self: Object, page: Object) void;
pub extern fn adw_navigation_view_pop(self: Object) c_int;
pub extern fn adw_navigation_view_replace(self: Object, pages: [*]const Object, n_pages: c_int) void;
pub extern fn adw_navigation_page_new(child: Object, title: [*:0]const u8) Object;
pub extern fn adw_navigation_page_set_title(self: Object, title: [*:0]const u8) void;

pub extern fn adw_status_page_new() Object;
pub extern fn adw_status_page_set_icon_name(self: Object, icon_name: [*:0]const u8) void;
pub extern fn adw_status_page_set_title(self: Object, title: [*:0]const u8) void;
pub extern fn adw_status_page_set_description(self: Object, description: [*:0]const u8) void;

// --- GTK widgets ---
pub extern fn gtk_window_new() Object;
pub extern fn gtk_window_set_child(window: Object, child: Object) void;
pub extern fn gtk_window_destroy(window: Object) void;
pub extern fn gtk_window_set_title(window: Object, title: [*:0]const u8) void;
pub extern fn gtk_window_set_default_size(window: Object, width: c_int, height: c_int) void;
pub extern fn gtk_window_present(window: Object) void;
pub extern fn gtk_widget_set_margin_top(widget: Object, margin: c_int) void;
pub extern fn gtk_widget_set_margin_bottom(widget: Object, margin: c_int) void;
pub extern fn gtk_widget_set_margin_start(widget: Object, margin: c_int) void;
pub extern fn gtk_widget_set_margin_end(widget: Object, margin: c_int) void;
pub extern fn gtk_widget_set_halign(widget: Object,@"align": c_int) void;
pub extern fn gtk_widget_set_valign(widget: Object, @"align": c_int) void;
pub extern fn gtk_widget_add_css_class(widget: Object, css_class: [*:0]const u8) void;
pub extern fn gtk_box_new(orientation: c_int, spacing: c_int) Object;
pub extern fn gtk_box_append(box: Object, child: Object) void;
pub extern fn gtk_box_remove(box: Object, child: Object) void;
pub extern fn gtk_widget_get_first_child(widget: Object) Object;
pub extern fn gtk_widget_get_next_sibling(widget: Object) Object;
pub extern fn gtk_label_new(str: ?[*:0]const u8) Object;
pub extern fn gtk_label_set_markup(label: Object, str: [*:0]const u8) void;
pub extern fn gtk_label_set_wrap(label: Object, wrap: c_int) void;
pub extern fn gtk_label_set_selectable(label: Object, setting: c_int) void;
pub extern fn gtk_button_new() Object;
pub extern fn gtk_button_new_with_label(label: [*:0]const u8) Object;
pub extern fn gtk_button_set_child(button: Object, child: Object) void;
pub extern fn gtk_separator_new(orientation: c_int) Object;
pub extern fn gtk_button_set_label(button: Object, label: [*:0]const u8) void;
pub extern fn gtk_editable_get_text(editable: Object) [*:0]const u8;
pub extern fn gtk_editable_set_text(editable: Object, text: [*:0]const u8) void;
pub extern fn gtk_editable_set_editable(editable: Object, is_editable: c_int) void;
pub extern fn gtk_entry_new() Object;
pub extern fn gtk_entry_set_placeholder_text(entry: Object, text: ?[*:0]const u8) void;

// AdwDialog — adaptive modal for the issue creator.
pub extern fn adw_dialog_new() Object;
pub extern fn adw_dialog_set_title(dialog: Object, title: [*:0]const u8) void;
pub extern fn adw_dialog_set_content_width(dialog: Object, width: c_int) void;
pub extern fn adw_dialog_set_content_height(dialog: Object, height: c_int) void;
pub extern fn adw_dialog_set_child(dialog: Object, child: Object) void;
pub extern fn adw_dialog_present(dialog: Object, parent: Object) void;
pub extern fn adw_dialog_close(dialog: Object) c_int;
pub extern fn adw_dialog_set_can_close(dialog: Object, can_close: c_int) void;
pub extern fn adw_header_bar_pack_start(self: Object, child: Object) void;

// GtkDropDown (status / priority / project pickers).
pub extern fn gtk_drop_down_new_from_strings(strings: [*:null]const ?[*:0]const u8) Object;
pub extern fn gtk_drop_down_get_selected(self: Object) c_uint;
pub extern fn gtk_drop_down_set_selected(self: Object, position: c_uint) void;

// GtkTextView + buffer (multi-line description). GtkTextIter is a fixed-size
// stack struct; we pass an over-sized aligned buffer for it.
pub const WRAP_WORD_CHAR: c_int = 3;
pub extern fn gtk_text_view_new() Object;
pub extern fn gtk_text_view_set_wrap_mode(text_view: Object, wrap_mode: c_int) void;
pub extern fn gtk_text_view_get_buffer(text_view: Object) Object;
pub extern fn gtk_text_buffer_set_text(buffer: Object, text: [*:0]const u8, len: c_int) void;
pub extern fn gtk_text_buffer_get_bounds(buffer: Object, start: ?*anyopaque, end: ?*anyopaque) void;
pub extern fn gtk_text_buffer_get_text(buffer: Object, start: ?*anyopaque, end: ?*anyopaque, include_hidden: c_int) ?[*:0]u8;
pub extern fn gtk_text_view_set_left_margin(text_view: Object, margin: c_int) void;
pub extern fn gtk_text_view_set_top_margin(text_view: Object, margin: c_int) void;

// Text tags + iters for the live-styling markdown editor.
pub extern fn gtk_text_buffer_create_tag(buffer: Object, tag_name: ?[*:0]const u8, ...) Object;
pub extern fn gtk_text_buffer_remove_all_tags(buffer: Object, start: ?*anyopaque, end: ?*anyopaque) void;
pub extern fn gtk_text_buffer_apply_tag_by_name(buffer: Object, name: [*:0]const u8, start: ?*anyopaque, end: ?*anyopaque) void;
pub extern fn gtk_text_buffer_get_iter_at_offset(buffer: Object, iter: ?*anyopaque, char_offset: c_int) void;
pub extern fn gtk_text_buffer_get_selection_bounds(buffer: Object, start: ?*anyopaque, end: ?*anyopaque) c_int;
pub extern fn gtk_text_buffer_insert(buffer: Object, iter: ?*anyopaque, text: [*]const u8, len: c_int) void;
pub extern fn gtk_text_buffer_insert_at_cursor(buffer: Object, text: [*]const u8, len: c_int) void;
pub extern fn gtk_text_buffer_delete(buffer: Object, start: ?*anyopaque, end: ?*anyopaque) void;
pub extern fn gtk_text_buffer_get_insert(buffer: Object) Object;
pub extern fn gtk_text_buffer_get_iter_at_mark(buffer: Object, iter: ?*anyopaque, mark: Object) void;
pub extern fn gtk_text_iter_set_line_offset(iter: ?*anyopaque, char_on_line: c_int) void;
// `get_slice` (unlike get_text) includes U+FFFC for child anchors, so restyle's
// char offsets stay aligned with the buffer when images are present.
pub extern fn gtk_text_buffer_get_slice(buffer: Object, start: ?*anyopaque, end: ?*anyopaque, include_hidden: c_int) ?[*:0]u8;
pub extern fn gtk_text_buffer_get_start_iter(buffer: Object, iter: ?*anyopaque) void;
pub extern fn gtk_text_buffer_get_end_iter(buffer: Object, iter: ?*anyopaque) void;
pub extern fn gtk_text_iter_is_end(iter: ?*anyopaque) c_int;
pub extern fn gtk_text_iter_forward_char(iter: ?*anyopaque) c_int;
pub extern fn gtk_text_iter_get_char(iter: ?*anyopaque) u32;
pub extern fn gtk_text_iter_get_child_anchor(iter: ?*anyopaque) Object;

// Inline image anchors in the editor.
pub extern fn gtk_text_buffer_create_child_anchor(buffer: Object, iter: ?*anyopaque) Object;
pub extern fn gtk_text_view_add_child_at_anchor(text_view: Object, child: Object, anchor: Object) void;

// Inline images: bytes → GdkTexture → GtkPicture.
pub const CONTENT_FIT_SCALE_DOWN: c_int = 3;
pub extern fn g_bytes_new(data: [*]const u8, size: usize) Object;
pub extern fn g_bytes_unref(bytes: Object) void;
pub extern fn gdk_texture_new_from_bytes(bytes: Object, @"error": ?*anyopaque) Object;
pub extern fn gtk_picture_new() Object;
pub extern fn gtk_picture_new_for_paintable(paintable: Object) Object;
pub extern fn gtk_picture_set_paintable(self: Object, paintable: Object) void;
pub extern fn gtk_picture_set_can_shrink(self: Object, can_shrink: c_int) void;
pub extern fn gtk_picture_set_content_fit(self: Object, content_fit: c_int) void;
pub extern fn g_object_ref(object: Object) Object;

// GtkFileDialog (async) for choosing an image to upload.
pub const AsyncReadyCallback = ?*const fn (source: Object, result: Object, data: gpointer) callconv(.c) void;
pub extern fn gtk_file_dialog_new() Object;
pub extern fn gtk_file_dialog_open(self: Object, parent: Object, cancellable: Object, callback: AsyncReadyCallback, user_data: gpointer) void;
pub extern fn gtk_file_dialog_open_finish(self: Object, result: Object, @"error": ?*anyopaque) Object;
pub extern fn g_file_get_path(file: Object) ?[*:0]u8;
pub extern fn gtk_widget_get_root(widget: Object) Object;
pub extern fn gtk_widget_set_visible(widget: Object, visible: c_int) void;
pub extern fn gtk_widget_get_visible(widget: Object) c_int;
pub extern fn gtk_widget_set_sensitive(widget: Object, sensitive: c_int) void;
pub extern fn gtk_widget_set_hexpand(widget: Object, expand: c_int) void;
pub extern fn gtk_widget_set_vexpand(widget: Object, expand: c_int) void;
pub extern fn gtk_scrolled_window_new() Object;
pub extern fn gtk_scrolled_window_set_child(scrolled_window: Object, child: Object) void;
pub extern fn gtk_scrolled_window_set_propagate_natural_height(scrolled_window: Object, propagate: c_int) void;
pub extern fn gtk_scrolled_window_set_max_content_height(scrolled_window: Object, height: c_int) void;
pub extern fn gtk_list_box_new() Object;
pub extern fn gtk_list_box_append(box: Object, child: Object) void;
pub extern fn gtk_list_box_remove_all(box: Object) void;
pub extern fn gtk_list_box_row_get_child(row: Object) Object;

// GSourceFunc timer / idle — drive UI refreshes on the main loop. g_idle_add is
// thread-safe, so sync threads use it to schedule a refresh.
pub extern fn g_timeout_add(interval_ms: c_uint, function: ?*const fn (gpointer) callconv(.c) c_int, data: gpointer) c_uint;

pub extern fn gtk_label_set_text(label: Object, str: [*:0]const u8) void;
// PangoEllipsizeMode.PANGO_ELLIPSIZE_END
pub const ELLIPSIZE_END: c_int = 3;
pub extern fn gtk_label_set_ellipsize(label: Object, mode: c_int) void;
pub extern fn gtk_label_set_xalign(label: Object, xalign: f32) void;
pub extern fn gtk_widget_set_size_request(widget: Object, width: c_int, height: c_int) void;
pub extern fn gtk_password_entry_new() Object;

// GObject per-widget data (used to stash an OIDC provider id on its button).
pub extern fn g_object_set_data_full(object: Object, key: [*:0]const u8, data: gpointer, destroy: ?*const fn (gpointer) callconv(.c) void) void;
pub extern fn g_object_get_data(object: Object, key: [*:0]const u8) gpointer;
pub extern fn g_free(mem: gpointer) void;
pub extern fn g_strdup(str: ?[*:0]const u8) ?[*:0]u8;

// --- read-only / focus on the text view (detail pane reuses the editor) ---
pub extern fn gtk_text_view_set_editable(text_view: Object, setting: c_int) void;
pub extern fn gtk_text_view_set_cursor_visible(text_view: Object, setting: c_int) void;
pub extern fn gtk_widget_grab_focus(widget: Object) c_int;
// GtkEventControllerFocus — save-on-blur for inline detail editing. The "leave"
// signal fires when focus leaves the widget.
pub extern fn gtk_event_controller_focus_new() Object;
pub extern fn gtk_widget_add_controller(widget: Object, controller: Object) void;
pub extern fn gtk_widget_set_tooltip_text(widget: Object, text: ?[*:0]const u8) void;
pub extern fn gtk_widget_remove_css_class(widget: Object, css_class: [*:0]const u8) void;

// --- GtkMenuButton + GtkPopover (icon-bearing status/priority/label/assignee
//     pickers, mirroring the web option dropdowns) ---
pub extern fn gtk_menu_button_new() Object;
pub extern fn gtk_menu_button_set_child(self: Object, child: Object) void;
pub extern fn gtk_menu_button_set_popover(self: Object, popover: Object) void;
pub extern fn gtk_menu_button_set_always_show_arrow(self: Object, setting: c_int) void;
// Lazily build the popover each time the button opens (used for the filter
// popover so it reflects current labels + filter state).
pub const MenuButtonCreatePopupFunc = ?*const fn (menu_button: Object, user_data: gpointer) callconv(.c) void;
pub extern fn gtk_menu_button_set_create_popup_func(self: Object, func: MenuButtonCreatePopupFunc, user_data: gpointer, destroy_notify: gpointer) void;
pub extern fn gtk_popover_new() Object;
pub extern fn gtk_popover_set_child(self: Object, child: Object) void;
pub extern fn gtk_popover_popdown(self: Object) void;

// --- segmented filter tabs + search ---
pub extern fn gtk_search_entry_new() Object;
pub extern fn gtk_check_button_new_with_label(label: [*:0]const u8) Object;
pub extern fn gtk_check_button_get_active(self: Object) c_int;
pub extern fn gtk_check_button_set_active(self: Object, setting: c_int) void;

// --- GtkCalendar + GDateTime (due-date picker) ---
pub extern fn gtk_calendar_new() Object;
pub extern fn gtk_calendar_get_date(self: Object) Object; // -> GDateTime* (owned)
pub extern fn gtk_calendar_select_day(self: Object, datetime: Object) void;
pub extern fn g_date_time_new_local(year: c_int, month: c_int, day: c_int, hour: c_int, minute: c_int, seconds: f64) Object;
pub extern fn g_date_time_get_year(datetime: Object) c_int;
pub extern fn g_date_time_get_month(datetime: Object) c_int;
pub extern fn g_date_time_get_day_of_month(datetime: Object) c_int;
pub extern fn g_date_time_unref(datetime: Object) void;

// --- GtkGLArea (embedded ghostty terminal surface) ---
// GtkGLApi flag: force desktop GL (not GLES) — ghostty's renderer needs GL 4.3.
pub const GL_API_GL: c_uint = 1 << 0;
pub extern fn gtk_gl_area_new() Object;
pub extern fn gtk_gl_area_set_auto_render(self: Object, auto_render: c_int) void;
pub extern fn gtk_gl_area_set_has_depth_buffer(self: Object, has_depth_buffer: c_int) void;
pub extern fn gtk_gl_area_set_has_stencil_buffer(self: Object, has_stencil_buffer: c_int) void;
pub extern fn gtk_gl_area_set_required_version(self: Object, major: c_int, minor: c_int) void;
pub extern fn gtk_gl_area_set_allowed_apis(self: Object, apis: c_uint) void;
pub extern fn gtk_gl_area_make_current(self: Object) void;
pub extern fn gtk_gl_area_get_error(self: Object) ?*anyopaque; // GError* (null = ok)
pub extern fn gtk_gl_area_queue_render(self: Object) void;
pub extern fn gtk_gl_area_attach_buffers(self: Object) void;

// create-context override: build the GL context ourselves (force desktop GL +
// 4.3) — GtkGLArea's own version/api constraints get rejected on some drivers.
pub const GDK_GL_API_GL: c_uint = 1 << 0;
pub extern fn gtk_widget_get_native(widget: Object) Object;
pub extern fn gtk_native_get_surface(self: Object) Object;
pub extern fn gdk_surface_create_gl_context(self: Object, @"error": ?*?*anyopaque) Object;
pub extern fn gdk_gl_context_set_allowed_apis(self: Object, apis: c_uint) void;
pub extern fn gdk_gl_context_set_required_version(self: Object, major: c_int, minor: c_int) void;

// --- widget geometry / focus (needed by the GL surface render/resize path) ---
pub extern fn gtk_widget_set_focusable(widget: Object, focusable: c_int) void;
pub extern fn gtk_widget_set_can_focus(widget: Object, can_focus: c_int) void;
pub extern fn gtk_widget_get_scale_factor(widget: Object) c_int;
pub extern fn gtk_widget_get_width(widget: Object) c_int;
pub extern fn gtk_widget_get_height(widget: Object) c_int;
pub extern fn gtk_widget_queue_draw(widget: Object) void;

// --- event controllers (keyboard / mouse / scroll / focus → ghostty surface) ---
pub extern fn gtk_event_controller_key_new() Object;
pub extern fn gtk_gesture_click_new() Object;
pub extern fn gtk_gesture_single_set_button(gesture: Object, button: c_uint) void;
pub extern fn gtk_gesture_single_get_current_button(gesture: Object) c_uint;
pub extern fn gtk_event_controller_motion_new() Object;
// GtkEventControllerScrollFlags: BOTH_AXES (VERTICAL|HORIZONTAL) | DISCRETE.
pub const SCROLL_BOTH_AXES: c_uint = (1 << 0) | (1 << 1);
pub const SCROLL_DISCRETE: c_uint = 1 << 2;
pub extern fn gtk_event_controller_scroll_new(flags: c_uint) Object;
// (gtk_event_controller_focus_new is declared above with the focus controls)

// --- application-wide CSS (label chips, group headers). GTK 4.12+ string API. ---
pub const STYLE_PROVIDER_PRIORITY_APPLICATION: c_uint = 600;
pub extern fn gtk_css_provider_new() Object;
pub extern fn gtk_css_provider_load_from_string(self: Object, string: [*:0]const u8) void;
pub extern fn gdk_display_get_default() Object;
pub extern fn gtk_style_context_add_provider_for_display(display: Object, provider: Object, priority: c_uint) void;
