//! The dedicated, resizable Preview pane — a GtkOverlay whose base is the live
//! preview surface (WebKitWebView for web, an X11 embed socket for android) and
//! whose overlay is the transparent annotation canvas. It sits in its own pane
//! (AdwOverlaySplitView) SEPARATE from the bottom terminal dock, with a header:
//! a target picker (named run targets grouped by platform), Build/Run/Stop, and
//! an Annotate toggle.
//!
//! Proportioned for a portrait phone (the android/iOS case); the web case fills.
//!
//! Main-thread only (it's pure GTK widget plumbing). The PreviewController
//! (preview.zig) owns the process lifecycle and calls mountSurface/clearSurface/
//! setStatus/setMessage here; the annotation overlay (annotation_overlay.zig)
//! draws into the overlay child.

const std = @import("std");
const gtk = @import("../gtk.zig");
const overlay = @import("annotation_overlay.zig");

/// Callbacks the host (app.zig) wires for the header buttons. Opaque ctx so the
/// panel needn't import AppState.
pub const Callbacks = struct {
    ctx: ?*anyopaque = null,
    on_run: ?*const fn (ctx: ?*anyopaque, target_index: usize) void = null,
    on_stop: ?*const fn (ctx: ?*anyopaque) void = null,
    on_target_changed: ?*const fn (ctx: ?*anyopaque, target_index: usize) void = null,
    // Annotate toggled ON: the host captures the current preview frame
    // (per-platform: adb screencap / webview snapshot) and sets it as the
    // overlay base via `overlayBase`/`overlaySetBase` before the user draws.
    on_annotate_begin: ?*const fn (ctx: ?*anyopaque) void = null,
};

/// One entry in the target picker (display metadata only — the platform selects
/// the embed mechanics, the name is what the user picks).
pub const PickerTarget = struct {
    name: []const u8,
    platform_label: []const u8, // "Web"/"Android"/"iOS"
};

pub const PreviewPanel = struct {
    gpa: std.mem.Allocator,
    root: gtk.Object, // the pane's top widget (a vertical box: header + overlay)
    status_label: gtk.Object,
    message_label: gtk.Object,
    picker_slot: gtk.Object, // holds the GtkDropDown (rebuilt in place)
    target_dropdown: gtk.Object, // GtkDropDown of target names
    run_btn: gtk.Object,
    stop_btn: gtk.Object,
    annotate_toggle: gtk.Object,

    surface_slot: gtk.Object, // base of the GtkOverlay; holds the live surface
    embed_socket: gtk.Object, // a GtkDrawingArea that realizes a native X11 window
    overlay_canvas: ?*overlay.AnnotationOverlay = null,
    annotate_toolbar: gtk.Object = null, // tool toggles + send (shown when annotating)
    current_surface: gtk.Object = null,

    callbacks: Callbacks = .{},
    target_count: usize = 0,

    /// Build the pane. `parent_window` is used to anchor the annotate send sheet.
    pub fn create(gpa: std.mem.Allocator, parent_window: gtk.Object) ?*PreviewPanel {
        const self = gpa.create(PreviewPanel) catch return null;

        const root = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
        gtk.gtk_widget_add_css_class(root, "exp-preview-pane");
        gtk.gtk_widget_set_hexpand(root, 1);
        gtk.gtk_widget_set_vexpand(root, 1);

        // --- header ---
        const header = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
        gtk.gtk_widget_set_margin_start(header, 8);
        gtk.gtk_widget_set_margin_end(header, 8);
        gtk.gtk_widget_set_margin_top(header, 6);
        gtk.gtk_widget_set_margin_bottom(header, 6);

        // The target picker lives in its own slot box so setTargets can rebuild
        // it in place (gtk_drop_down's string model is fixed at construction).
        const picker_slot = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 0);
        const empty = [_:null]?[*:0]const u8{null};
        const dropdown = gtk.gtk_drop_down_new_from_strings(&empty);
        gtk.gtk_widget_set_tooltip_text(dropdown, "Run target");
        _ = gtk.g_signal_connect_data(dropdown, "notify::selected", @ptrCast(&onTargetChanged), self, null, 0);
        gtk.gtk_box_append(picker_slot, dropdown);
        gtk.gtk_box_append(header, picker_slot);

        const run_btn = gtk.gtk_button_new_with_label("Run");
        gtk.gtk_widget_add_css_class(run_btn, "suggested-action");
        _ = gtk.g_signal_connect_data(run_btn, "clicked", @ptrCast(&onRunClicked), self, null, 0);
        gtk.gtk_box_append(header, run_btn);

        const stop_btn = gtk.gtk_button_new_with_label("Stop");
        gtk.gtk_widget_add_css_class(stop_btn, "flat");
        gtk.gtk_widget_set_sensitive(stop_btn, 0);
        _ = gtk.g_signal_connect_data(stop_btn, "clicked", @ptrCast(&onStopClicked), self, null, 0);
        gtk.gtk_box_append(header, stop_btn);

        const status = gtk.gtk_label_new("Idle");
        gtk.gtk_widget_add_css_class(status, "dim-label");
        gtk.gtk_widget_set_hexpand(status, 1);
        gtk.gtk_widget_set_halign(status, gtk.ALIGN_START);
        gtk.gtk_box_append(header, status);

        const annotate = gtk.gtk_toggle_button_new_with_label("Annotate");
        gtk.gtk_widget_set_tooltip_text(annotate, "Draw on the preview and file an issue");
        gtk.gtk_widget_set_sensitive(annotate, 0);
        _ = gtk.g_signal_connect_data(annotate, "toggled", @ptrCast(&onAnnotateToggled), self, null, 0);
        gtk.gtk_box_append(header, annotate);

        gtk.gtk_box_append(root, header);

        // --- the message line (errors / hints) ---
        const message = gtk.gtk_label_new("");
        gtk.gtk_widget_add_css_class(message, "dim-label");
        gtk.gtk_widget_set_halign(message, gtk.ALIGN_START);
        gtk.gtk_widget_set_margin_start(message, 8);
        gtk.gtk_label_set_wrap(message, 1);
        gtk.gtk_label_set_xalign(message, 0);
        gtk.gtk_box_append(root, message);

        // --- the overlay: base surface slot + transparent annotation canvas ---
        const ov = gtk.gtk_overlay_new();
        gtk.gtk_widget_set_hexpand(ov, 1);
        gtk.gtk_widget_set_vexpand(ov, 1);

        const slot = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
        gtk.gtk_widget_set_hexpand(slot, 1);
        gtk.gtk_widget_set_vexpand(slot, 1);
        // Portrait-phone min size so the embed never mounts at 0×0 (the lazy-size
        // discipline the terminal dock relies on) and a phone preview reads right.
        gtk.gtk_widget_set_size_request(slot, 360, 640);
        gtk.gtk_overlay_set_child(ov, slot);

        // The android embed socket: a GtkDrawingArea realizes a native surface we
        // can reparent the emulator window into. Hidden until an android run.
        const socket = gtk.gtk_drawing_area_new();
        gtk.gtk_widget_set_hexpand(socket, 1);
        gtk.gtk_widget_set_vexpand(socket, 1);
        gtk.gtk_widget_set_visible(socket, 0);

        self.* = .{
            .gpa = gpa,
            .root = root,
            .status_label = status,
            .message_label = message,
            .picker_slot = picker_slot,
            .target_dropdown = dropdown,
            .run_btn = run_btn,
            .stop_btn = stop_btn,
            .annotate_toggle = annotate,
            .surface_slot = slot,
            .embed_socket = socket,
        };

        // The annotation overlay (transparent canvas) sits above the surface; its
        // toolbar (tool toggles + undo/clear + send) is appended UNDER the header
        // (before the overlay) so the layout reads header → toolbar → preview.
        // Hidden until annotating.
        if (overlay.AnnotationOverlay.create(gpa, parent_window)) |oc| {
            self.overlay_canvas = oc;
            gtk.gtk_widget_set_visible(oc.area, 0);
            gtk.gtk_overlay_add_overlay(ov, oc.area);
            const toolbar = oc.buildToolbar();
            gtk.gtk_widget_set_visible(toolbar, 0);
            self.annotate_toolbar = toolbar;
            gtk.gtk_box_append(root, toolbar);
        }
        // The overlay (live surface + annotation canvas) fills the rest.
        gtk.gtk_box_append(root, ov);

        return self;
    }

    pub fn destroy(self: *PreviewPanel) void {
        if (self.overlay_canvas) |oc| oc.destroy();
        self.gpa.destroy(self);
    }

    pub fn setCallbacks(self: *PreviewPanel, cb: Callbacks) void {
        self.callbacks = cb;
    }

    /// The annotation overlay (so the host can set its base frame + report
    /// context). Null only if the overlay couldn't be created.
    pub fn overlayCanvas(self: *PreviewPanel) ?*overlay.AnnotationOverlay {
        return self.overlay_canvas;
    }

    /// Replace the target picker with named targets (grouped visually by platform
    /// in the label text: "Web — Android — …"). Indices are stable for the host's
    /// run callback.
    pub fn setTargets(self: *PreviewPanel, targets: []const PickerTarget) void {
        var arena = std.heap.ArenaAllocator.init(self.gpa);
        defer arena.deinit();
        const a = arena.allocator();
        var strs = std.ArrayList(?[*:0]const u8).empty;
        for (targets) |t| {
            const s = std.fmt.allocPrintSentinel(a, "{s} · {s}", .{ t.name, t.platform_label }, 0) catch continue;
            strs.append(a, s.ptr) catch {};
        }
        strs.append(a, null) catch {}; // null-terminate for gtk_drop_down_new_from_strings
        const dd = gtk.gtk_drop_down_new_from_strings(@ptrCast(strs.items.ptr));
        gtk.gtk_widget_set_tooltip_text(dd, "Run target");
        _ = gtk.g_signal_connect_data(dd, "notify::selected", @ptrCast(&onTargetChanged), self, null, 0);
        // Rebuild the picker in place: clear the slot, append the fresh dropdown.
        gtk.gtk_box_remove(self.picker_slot, self.target_dropdown);
        gtk.gtk_box_append(self.picker_slot, dd);
        self.target_dropdown = dd;
        self.target_count = targets.len;
    }

    /// Currently selected target index.
    pub fn selectedTarget(self: *PreviewPanel) usize {
        return @intCast(gtk.gtk_drop_down_get_selected(self.target_dropdown));
    }

    pub fn selectTarget(self: *PreviewPanel, index: usize) void {
        gtk.gtk_drop_down_set_selected(self.target_dropdown, @intCast(index));
    }

    pub fn setStatus(self: *PreviewPanel, text: [*:0]const u8) void {
        gtk.gtk_label_set_text(self.status_label, text);
    }

    pub fn setMessage(self: *PreviewPanel, text: []const u8) void {
        var buf: [512]u8 = undefined;
        const z = std.fmt.bufPrintZ(&buf, "{s}", .{text}) catch return;
        gtk.gtk_label_set_text(self.message_label, z.ptr);
    }

    /// Mount the live preview surface (webview) into the base slot. Replaces any
    /// prior surface. Reveals the annotate toggle + Stop.
    pub fn mountSurface(self: *PreviewPanel, surface: gtk.Object) void {
        self.clearSurfaceWidget();
        gtk.gtk_widget_set_hexpand(surface, 1);
        gtk.gtk_widget_set_vexpand(surface, 1);
        gtk.gtk_box_append(self.surface_slot, surface);
        self.current_surface = surface;
        gtk.gtk_widget_set_sensitive(self.stop_btn, 1);
        gtk.gtk_widget_set_sensitive(self.annotate_toggle, 1);
    }

    /// Reveal + return the android embed socket (a native-surface GtkDrawingArea)
    /// so the controller can reparent the emulator window into it. Mounting it is
    /// the same path as a webview surface.
    pub fn mountEmbedSocket(self: *PreviewPanel) void {
        self.clearSurfaceWidget();
        gtk.gtk_widget_set_visible(self.embed_socket, 1);
        gtk.gtk_box_append(self.surface_slot, self.embed_socket);
        self.current_surface = self.embed_socket;
        gtk.gtk_widget_set_sensitive(self.stop_btn, 1);
        gtk.gtk_widget_set_sensitive(self.annotate_toggle, 1);
    }

    /// The X11 window id of the embed socket's native surface (reparent target).
    /// Returns null when not on X11 / not yet realized.
    pub fn embedSurfaceXid(self: *PreviewPanel) ?gtk.XID {
        const native = gtk.gtk_widget_get_native(self.embed_socket);
        if (native == null) return null;
        const surface = gtk.gtk_native_get_surface(native);
        if (surface == null) return null;
        return gtk.gdk_x11_surface_get_xid(surface);
    }

    /// Tear the live surface out (controller stop()). Idempotent.
    pub fn clearSurface(self: *PreviewPanel) void {
        self.clearSurfaceWidget();
        if (self.overlay_canvas) |oc| {
            gtk.gtk_widget_set_visible(oc.area, 0);
            oc.clear();
        }
        gtk.gtk_toggle_button_set_active(self.annotate_toggle, 0);
        gtk.gtk_widget_set_sensitive(self.annotate_toggle, 0);
        gtk.gtk_widget_set_sensitive(self.stop_btn, 0);
    }

    fn clearSurfaceWidget(self: *PreviewPanel) void {
        if (self.current_surface != null) {
            if (self.current_surface == self.embed_socket) {
                gtk.gtk_widget_set_visible(self.embed_socket, 0);
                gtk.gtk_box_remove(self.surface_slot, self.embed_socket);
            } else {
                gtk.gtk_box_remove(self.surface_slot, self.current_surface);
            }
            self.current_surface = null;
        }
    }

    /// Render the iOS "needs a Mac" empty state in the surface slot.
    pub fn showNeedsMac(self: *PreviewPanel) void {
        self.clearSurfaceWidget();
        const page = gtk.adw_status_page_new();
        gtk.adw_status_page_set_icon_name(page, "computer-symbolic");
        gtk.adw_status_page_set_title(page, "iOS preview needs a Mac");
        gtk.adw_status_page_set_description(page, "Apple licensing forbids running the iOS Simulator on Linux. Open this project on the macOS app to preview an iOS target; a remote-Mac tunnel is planned.");
        gtk.gtk_widget_set_hexpand(page, 1);
        gtk.gtk_widget_set_vexpand(page, 1);
        gtk.gtk_box_append(self.surface_slot, page);
        self.current_surface = page;
        gtk.gtk_widget_set_sensitive(self.stop_btn, 0);
        gtk.gtk_widget_set_sensitive(self.annotate_toggle, 0);
    }

    // --- header signal handlers ---

    fn onRunClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *PreviewPanel = @ptrCast(@alignCast(data));
        if (self.callbacks.on_run) |cb| cb(self.callbacks.ctx, self.selectedTarget());
    }
    fn onStopClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *PreviewPanel = @ptrCast(@alignCast(data));
        if (self.callbacks.on_stop) |cb| cb(self.callbacks.ctx);
    }
    fn onTargetChanged(_: gtk.Object, _: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *PreviewPanel = @ptrCast(@alignCast(data));
        if (self.callbacks.on_target_changed) |cb| cb(self.callbacks.ctx, self.selectedTarget());
    }
    fn onAnnotateToggled(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *PreviewPanel = @ptrCast(@alignCast(data));
        const active = gtk.gtk_toggle_button_get_active(btn) != 0;
        if (self.annotate_toolbar != null) gtk.gtk_widget_set_visible(self.annotate_toolbar, if (active) 1 else 0);
        if (self.overlay_canvas) |oc| {
            gtk.gtk_widget_set_visible(oc.area, if (active) 1 else 0);
            if (active) {
                oc.beginSession();
                // Ask the host to capture the current preview frame as the
                // annotation base (per-platform: adb screencap / webview
                // snapshot) and call setBase on the overlay. Done async so the
                // screencap doesn't block; until it lands, the overlay just has
                // no base (drawing is gated on image_w > 0).
                if (self.callbacks.on_annotate_begin) |cb| cb(self.callbacks.ctx);
            } else {
                oc.clear();
            }
        }
    }
};
