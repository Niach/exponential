//! IDE-style collapsible bottom terminal dock. Wraps the content nav in a
//! vertical GtkPaned; the bottom pane hosts the agent's terminal under a small
//! header (title + collapse). Libghostty's surface inits lazily only at a
//! nonzero size, so we ONLY reveal the dock at a real split position and give
//! the dock a nonzero min height — never mount a terminal into a 0-height slot.

const std = @import("std");
const gtk = @import("gtk.zig");

pub const TerminalDock = struct {
    gpa: std.mem.Allocator,
    paned: gtk.Object, // vertical: content (start) / dock_box (end)
    dock_box: gtk.Object, // header + term_slot
    title: gtk.Object, // header label
    term_slot: gtk.Object, // holds the current terminal widget
    current_term: gtk.Object = null,
    collapsed: bool = true,

    /// Build the dock around `content` (the existing content widget). Returns the
    /// GtkPaned to mount where `content` used to live. Starts collapsed.
    pub fn create(gpa: std.mem.Allocator, content: gtk.Object) ?*TerminalDock {
        const self = gpa.create(TerminalDock) catch return null;

        const paned = gtk.gtk_paned_new(gtk.ORIENTATION_VERTICAL);
        gtk.gtk_paned_set_start_child(paned, content);
        gtk.gtk_paned_set_resize_start_child(paned, 1);
        gtk.gtk_paned_set_shrink_start_child(paned, 0);

        const dock_box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
        gtk.gtk_widget_add_css_class(dock_box, "exp-terminal-dock");
        // Nonzero min height so the embedded ghostty GLArea always inits.
        gtk.gtk_widget_set_size_request(dock_box, -1, 200);

        // Header: title + a collapse button.
        const header = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
        gtk.gtk_widget_set_margin_start(header, 8);
        gtk.gtk_widget_set_margin_end(header, 8);
        gtk.gtk_widget_set_margin_top(header, 4);
        gtk.gtk_widget_set_margin_bottom(header, 4);
        const title = gtk.gtk_label_new("Terminal");
        gtk.gtk_widget_add_css_class(title, "caption-heading");
        gtk.gtk_widget_set_halign(title, gtk.ALIGN_START);
        gtk.gtk_widget_set_hexpand(title, 1);
        gtk.gtk_box_append(header, title);
        const collapse_btn = gtk.gtk_button_new_with_label("\u{25BC}"); // ▼
        gtk.gtk_widget_add_css_class(collapse_btn, "flat");
        gtk.gtk_widget_set_tooltip_text(collapse_btn, "Hide the terminal");
        gtk.gtk_box_append(header, collapse_btn);
        gtk.gtk_box_append(dock_box, header);

        const term_slot = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
        gtk.gtk_widget_set_vexpand(term_slot, 1);
        gtk.gtk_box_append(dock_box, term_slot);

        gtk.gtk_paned_set_end_child(paned, dock_box);
        gtk.gtk_paned_set_resize_end_child(paned, 0);
        gtk.gtk_paned_set_shrink_end_child(paned, 0);

        self.* = .{
            .gpa = gpa,
            .paned = paned,
            .dock_box = dock_box,
            .title = title,
            .term_slot = term_slot,
        };
        gtk.gtk_widget_set_visible(dock_box, 0); // collapsed until a run mounts
        _ = gtk.g_signal_connect_data(collapse_btn, "clicked", @ptrCast(&onCollapse), self, null, 0);
        return self;
    }

    fn onCollapse(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *TerminalDock = @ptrCast(@alignCast(data));
        self.collapse();
    }

    /// Mount a terminal widget + reveal the dock at a real height (so ghostty's
    /// lazily-initialised surface gets a nonzero size). Replaces any prior run.
    pub fn mountTerminal(self: *TerminalDock, term: gtk.Object, title_text: [*:0]const u8) void {
        if (self.current_term) |t| gtk.gtk_box_remove(self.term_slot, t);
        gtk.gtk_box_append(self.term_slot, term);
        self.current_term = term;
        gtk.gtk_label_set_text(self.title, title_text);
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

    pub fn setTitle(self: *TerminalDock, t: [*:0]const u8) void {
        gtk.gtk_label_set_text(self.title, t);
    }
};

/// C-style hook so the agent manager (which doesn't import this module's types)
/// can mount a finished/running terminal into the dock by opaque pointer.
pub fn mountForManager(dock: *anyopaque, term: gtk.Object, title: [*:0]const u8) void {
    const self: *TerminalDock = @ptrCast(@alignCast(dock));
    self.mountTerminal(term, title);
}
