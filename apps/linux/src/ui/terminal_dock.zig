//! IDE-style collapsible bottom terminal dock (masterplan §4d). Wraps the
//! content nav in a vertical GtkPaned; the bottom pane hosts an `AdwTabView`
//! of embedded terminals — one tab per coding session (keyed by
//! `coding_sessions.id`) or run-target launch, each owning its own ghostty
//! surface + child process + worktree. Nothing is shared between tabs.
//!
//! Detach: a tab can be popped out into its own window (drag the tab out of
//! the strip, or the header's "move to new window" button). Either way the
//! page is TRANSFERRED between AdwTabViews, which reparents the existing
//! child widget — the ghostty surface is NEVER destroyed/recreated (that
//! would kill the claude child; `terminal.zig`'s realize/unrealize handlers
//! migrate the GL context across windows).
//!
//! Ghostty gotchas honored: a surface inits lazily only at NONZERO size, so
//! the dock keeps a nonzero min height, reveals itself at a real split
//! position when a tab mounts, and detached windows are presented BEFORE the
//! page transfer so the reparented GLArea realizes at a real allocation.

const std = @import("std");
const gtk = @import("gtk.zig");
const tabs = @import("tab_registry.zig");

pub const TerminalDock = struct {
    gpa: std.mem.Allocator,
    app: gtk.Object, // the GtkApplication — detached windows attach to it
    paned: gtk.Object, // vertical: content (start) / dock_box (end)
    dock_box: gtk.Object, // header (tab strip) + tab view
    tab_view: gtk.Object, // the docked AdwTabView
    registry: tabs.TabRegistry,
    detached: std.ArrayListUnmanaged(*DetachedWindow) = .empty,
    collapsed: bool = true,
    // Main-view handler ids — disconnected in destroy() so the late GTK
    // teardown (sign-out swaps the window content, destroying these widgets
    // afterwards) can never fire into a freed dock struct.
    sig_attached: c_ulong = 0,
    sig_page_detached: c_ulong = 0,
    sig_create_window: c_ulong = 0,

    /// One popped-out terminal window: its own AdwTabView the dock transfers
    /// pages into/out of. All bookkeeping is released (releaseDetached) while
    /// the window is still alive, so every disconnect targets a live instance.
    const DetachedWindow = struct {
        dock: *TerminalDock,
        window: gtk.Object,
        view: gtk.Object,
        sig_attached: c_ulong = 0,
        sig_page_detached: c_ulong = 0,
        sig_create_window: c_ulong = 0,
        sig_close_request: c_ulong = 0,
    };

    /// Build the dock around `content` (the existing content widget). Returns
    /// the dock; mount `dock.paned` where `content` used to live. Starts
    /// collapsed. `app` is the GtkApplication detached windows attach to.
    pub fn create(gpa: std.mem.Allocator, app: gtk.Object, content: gtk.Object) ?*TerminalDock {
        const self = gpa.create(TerminalDock) catch return null;

        const paned = gtk.gtk_paned_new(gtk.ORIENTATION_VERTICAL);
        gtk.gtk_paned_set_start_child(paned, content);
        gtk.gtk_paned_set_resize_start_child(paned, 1);
        gtk.gtk_paned_set_shrink_start_child(paned, 0);

        const dock_box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
        gtk.gtk_widget_add_css_class(dock_box, "exp-terminal-dock");
        // Nonzero min height so the embedded ghostty GLArea always inits.
        gtk.gtk_widget_set_size_request(dock_box, -1, 200);

        const tab_view = gtk.adw_tab_view_new();
        gtk.gtk_widget_set_vexpand(tab_view, 1);

        // Header: the tab strip + pop-out + collapse.
        const header = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
        gtk.gtk_widget_set_margin_start(header, 8);
        gtk.gtk_widget_set_margin_end(header, 8);
        gtk.gtk_widget_set_margin_top(header, 4);
        gtk.gtk_widget_set_margin_bottom(header, 4);
        const bar = gtk.adw_tab_bar_new();
        gtk.adw_tab_bar_set_view(bar, tab_view);
        gtk.adw_tab_bar_set_autohide(bar, 0);
        gtk.gtk_widget_set_hexpand(bar, 1);
        gtk.gtk_box_append(header, bar);
        const popout_btn = gtk.gtk_button_new();
        gtk.gtk_button_set_icon_name(popout_btn, "window-new-symbolic");
        gtk.gtk_widget_add_css_class(popout_btn, "flat");
        gtk.gtk_widget_set_tooltip_text(popout_btn, "Move the current tab to a new window");
        gtk.gtk_box_append(header, popout_btn);
        const collapse_btn = gtk.gtk_button_new_with_label("\u{25BC}"); // ▼
        gtk.gtk_widget_add_css_class(collapse_btn, "flat");
        gtk.gtk_widget_set_tooltip_text(collapse_btn, "Hide the terminal");
        gtk.gtk_box_append(header, collapse_btn);
        gtk.gtk_box_append(dock_box, header);
        gtk.gtk_box_append(dock_box, tab_view);

        gtk.gtk_paned_set_end_child(paned, dock_box);
        gtk.gtk_paned_set_resize_end_child(paned, 0);
        gtk.gtk_paned_set_shrink_end_child(paned, 0);

        self.* = .{
            .gpa = gpa,
            .app = app,
            .paned = paned,
            .dock_box = dock_box,
            .tab_view = tab_view,
            .registry = tabs.TabRegistry.init(gpa),
        };
        gtk.gtk_widget_set_visible(dock_box, 0); // collapsed until a run mounts
        _ = gtk.g_signal_connect_data(collapse_btn, "clicked", @ptrCast(&onCollapse), self, null, 0);
        _ = gtk.g_signal_connect_data(popout_btn, "clicked", @ptrCast(&onPopOut), self, null, 0);
        self.sig_attached = gtk.g_signal_connect_data(tab_view, "page-attached", @ptrCast(&onMainPageAttached), self, null, 0);
        self.sig_page_detached = gtk.g_signal_connect_data(tab_view, "page-detached", @ptrCast(&onMainPageDetached), self, null, 0);
        self.sig_create_window = gtk.g_signal_connect_data(tab_view, "create-window", @ptrCast(&onMainCreateWindow), self, null, 0);
        return self;
    }

    /// Sign-out teardown. Destroys the detached windows (killing their
    /// sessions, exactly like closing their tabs), then disarms every handler
    /// that references this struct — the main window's dock widgets die later
    /// when the window content is swapped, and must not fire into freed memory.
    pub fn destroy(self: *TerminalDock) void {
        while (self.detached.pop()) |dw| {
            const win = dw.window;
            self.releaseDetached(win); // frees dw (still marked on the window)
            gtk.gtk_window_destroy(win);
        }
        gtk.g_signal_handler_disconnect(self.tab_view, self.sig_attached);
        gtk.g_signal_handler_disconnect(self.tab_view, self.sig_page_detached);
        gtk.g_signal_handler_disconnect(self.tab_view, self.sig_create_window);
        // Remaining (docked) tabs: disarm their registry hooks; the widgets
        // themselves die with the window content.
        for (self.registry.entries.items) |entry| {
            if (entry.destroy_handler != 0)
                gtk.g_signal_handler_disconnect(entry.widget, entry.destroy_handler);
        }
        self.registry.deinit();
        self.detached.deinit(self.gpa);
        self.gpa.destroy(self);
    }

    fn onCollapse(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        self.collapse();
    }

    /// Add a terminal tab keyed by `key` (a `coding_sessions.id` or run-target
    /// id; empty ⇒ the registry generates one), reveal the dock at a real
    /// height (so ghostty's lazily-initialised surface gets a nonzero size),
    /// and select it. Concurrent tabs coexist — nothing is replaced.
    pub fn addTab(self: *TerminalDock, key: []const u8, kind: tabs.Kind, term: gtk.Object, title_text: [*:0]const u8) void {
        const entry = self.registry.add(key, kind, term);
        const page = gtk.adw_tab_view_append(self.tab_view, term);
        gtk.adw_tab_page_set_title(page, title_text);
        // Keep the registry honest no matter how the tab dies (close button,
        // detached-window close, kill-switch teardown): hook the child widget's
        // destroy. Disarmed in destroy() for widgets that outlive the dock.
        if (entry) |e| {
            e.destroy_handler = gtk.g_signal_connect_data(term, "destroy", @ptrCast(&onTabChildDestroy), self, null, 0);
        }
        gtk.adw_tab_view_set_selected_page(self.tab_view, page);
        self.expand();
    }

    pub fn expand(self: *TerminalDock) void {
        gtk.gtk_widget_set_visible(self.dock_box, 1);
        // Split so the bottom pane is ~260px tall (clamped to a real, nonzero
        // height even before the paned has its final allocation).
        const total = gtk.gtk_widget_get_height(self.paned);
        const pos: c_int = if (total > 360) total - 260 else 240;
        gtk.gtk_paned_set_position(self.paned, pos);
        self.collapsed = false;
    }

    pub fn collapse(self: *TerminalDock) void {
        gtk.gtk_widget_set_visible(self.dock_box, 0);
        self.collapsed = true;
    }

    // --- tab lifecycle bookkeeping -----------------------------------------

    fn onTabChildDestroy(widget: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        _ = self.registry.removeByWidget(widget);
    }

    fn onMainPageAttached(_: gtk.Object, page: gtk.Object, _: c_int, data: gtk.gpointer) callconv(.c) void {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        _ = self.registry.markDocked(gtk.adw_tab_page_get_child(page));
    }

    fn onMainPageDetached(_: gtk.Object, _: gtk.Object, _: c_int, data: gtk.gpointer) callconv(.c) void {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        // Last docked tab closed or dragged out — nothing left to show.
        if (gtk.adw_tab_view_get_n_pages(self.tab_view) == 0) self.collapse();
    }

    /// AdwTabView asks for a window to drop a dragged-out tab into.
    fn onMainCreateWindow(_: gtk.Object, data: gtk.gpointer) callconv(.c) gtk.Object {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        const dw = self.newDetachedWindow() orelse return null;
        return dw.view;
    }

    // --- detach to a new window (§4d: reparent, never recreate) -------------

    fn onPopOut(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        const page = gtk.adw_tab_view_get_selected_page(self.tab_view);
        if (page == null) return;
        const dw = self.newDetachedWindow() orelse return;
        // Transfer moves the page between views by REPARENTING the child — the
        // ghostty surface (and its claude child) survives the move.
        gtk.adw_tab_view_transfer_page(self.tab_view, page, dw.view, 0);
    }

    /// Build + present an empty detached terminal window. Presented BEFORE any
    /// page transfer so the reparented GLArea realizes at a nonzero size.
    fn newDetachedWindow(self: *TerminalDock) ?*DetachedWindow {
        const dw = self.gpa.create(DetachedWindow) catch return null;

        const win = gtk.adw_application_window_new(self.app);
        gtk.gtk_window_set_title(win, "Terminal — Exponential");
        gtk.gtk_window_set_default_size(win, 960, 640);
        const view = gtk.adw_tab_view_new();
        gtk.gtk_widget_set_vexpand(view, 1);
        const bar = gtk.adw_tab_bar_new();
        gtk.adw_tab_bar_set_view(bar, view);
        const toolbar = gtk.adw_toolbar_view_new();
        const header = gtk.adw_header_bar_new();
        gtk.adw_header_bar_set_title_widget(header, bar);
        gtk.adw_toolbar_view_add_top_bar(toolbar, header);
        gtk.adw_toolbar_view_set_content(toolbar, view);
        gtk.adw_application_window_set_content(win, toolbar);

        dw.* = .{ .dock = self, .window = win, .view = view };
        // The window's "exp-dock-dw" data is the liveness marker every cleanup
        // path checks (releaseDetached is idempotent through it).
        gtk.g_object_set_data_full(win, "exp-dock-dw", dw, null);
        dw.sig_attached = gtk.g_signal_connect_data(view, "page-attached", @ptrCast(&onDetachedPageAttached), dw, null, 0);
        dw.sig_page_detached = gtk.g_signal_connect_data(view, "page-detached", @ptrCast(&onDetachedPageDetached), dw, null, 0);
        dw.sig_create_window = gtk.g_signal_connect_data(view, "create-window", @ptrCast(&onDetachedCreateWindow), dw, null, 0);
        dw.sig_close_request = gtk.g_signal_connect_data(win, "close-request", @ptrCast(&onDetachedCloseRequest), dw, null, 0);
        self.detached.append(self.gpa, dw) catch {
            // Untracked-by-list is survivable: close-request still releases it.
        };

        gtk.gtk_window_present(win);
        return dw;
    }

    /// Disarm + free a detached window's bookkeeping. Idempotent (the window's
    /// "exp-dock-dw" data is the marker) and always runs BEFORE the window is
    /// destroyed, so every disconnect targets a live instance and no signal can
    /// fire into the freed struct afterwards.
    fn releaseDetached(self: *TerminalDock, win: gtk.Object) void {
        const raw = gtk.g_object_get_data(win, "exp-dock-dw") orelse return;
        const dw: *DetachedWindow = @ptrCast(@alignCast(raw));
        gtk.g_object_set_data_full(win, "exp-dock-dw", null, null);
        gtk.g_signal_handler_disconnect(dw.view, dw.sig_attached);
        gtk.g_signal_handler_disconnect(dw.view, dw.sig_page_detached);
        gtk.g_signal_handler_disconnect(dw.view, dw.sig_create_window);
        gtk.g_signal_handler_disconnect(dw.window, dw.sig_close_request);
        for (self.detached.items, 0..) |item, i| {
            if (item == dw) {
                _ = self.detached.swapRemove(i);
                break;
            }
        }
        self.gpa.destroy(dw);
    }

    fn onDetachedPageAttached(_: gtk.Object, page: gtk.Object, _: c_int, data: gtk.gpointer) callconv(.c) void {
        const dw: *DetachedWindow = @ptrCast(@alignCast(data));
        _ = dw.dock.registry.markWindow(gtk.adw_tab_page_get_child(page), dw.window);
        gtk.gtk_window_set_title(dw.window, gtk.adw_tab_page_get_title(page));
    }

    fn onDetachedPageDetached(_: gtk.Object, _: gtk.Object, _: c_int, data: gtk.gpointer) callconv(.c) void {
        const dw: *DetachedWindow = @ptrCast(@alignCast(data));
        if (gtk.adw_tab_view_get_n_pages(dw.view) != 0) return;
        // Empty window → close it, but NOT from inside the tab-view signal.
        // The idle holds a window ref and re-checks the liveness marker, so a
        // user-close/sign-out racing in between can't double-destroy.
        _ = gtk.g_object_ref(dw.window);
        _ = gtk.g_idle_add(@ptrCast(&closeEmptyWindowIdle), dw.window);
    }

    fn closeEmptyWindowIdle(data: gtk.gpointer) callconv(.c) c_int {
        const win: gtk.Object = data;
        if (gtk.g_object_get_data(win, "exp-dock-dw")) |raw| {
            const dw: *DetachedWindow = @ptrCast(@alignCast(raw));
            // A page may have been dropped back in before this idle ran.
            if (gtk.adw_tab_view_get_n_pages(dw.view) == 0) {
                const dock = dw.dock;
                dock.releaseDetached(win);
                gtk.gtk_window_destroy(win);
            }
        }
        gtk.g_object_unref(win);
        return 0; // G_SOURCE_REMOVE
    }

    fn onDetachedCreateWindow(_: gtk.Object, data: gtk.gpointer) callconv(.c) gtk.Object {
        const dw: *DetachedWindow = @ptrCast(@alignCast(data));
        const new_dw = dw.dock.newDetachedWindow() orelse return null;
        return new_dw.view;
    }

    /// The user closes a detached window: release the bookkeeping while the
    /// widgets are alive, then let the destroy proceed — the tabs' children
    /// die with the window (their own destroy hooks clean the registry, and
    /// each terminal's teardown ends its session, same as closing the tab).
    fn onDetachedCloseRequest(win: gtk.Object, data: gtk.gpointer) callconv(.c) c_int {
        const dw: *DetachedWindow = @ptrCast(@alignCast(data));
        dw.dock.releaseDetached(win);
        return 0; // proceed with the close
    }
};
