//! Small reusable GTK widgets that give the tracker its web-parity look:
//! coloured status/priority icon labels, label chips (rounded pills), and the
//! one-time CSS for them. Built on the GTK-free `format.zig` option tables.

const std = @import("std");
const gtk = @import("gtk.zig");
const format = @import("format.zig");

/// Install application-wide CSS once: the web token layer (the dark-theme
/// custom properties from apps/web/src/styles.css, verbatim OKLCH — GTK 4.16+
/// parses var()/oklch()/color-mix()), an Adwaita named-colour bridge so stock
/// widgets pick up the same palette, shadcn-metric `exp-btn` button classes,
/// and the exp-* component classes. Safe to call multiple times but intended
/// for startup.
pub fn applyCss() void {
    // The web app is dark-only — force the dark scheme regardless of the
    // desktop's light/dark preference (Adwaita is initialised by now).
    gtk.adw_style_manager_set_color_scheme(gtk.adw_style_manager_get_default(), gtk.ADW_COLOR_SCHEME_FORCE_DARK);

    const provider = gtk.gtk_css_provider_new();
    gtk.gtk_css_provider_load_from_string(provider,
    // --- Token layer: the `.dark` theme block of apps/web/src/styles.css,
    //     verbatim, as GTK CSS custom properties. Sizing tokens use the
    //     nominal shadcn px scale (1rem = 16px): --radius 0.625rem → 10px,
    //     text-sm → 14px, spacing n → n×4px. Everything below consumes these
    //     via var(); update HERE when the web tokens change. ---
        \\:root {
        \\  --background: oklch(0.145 0 0);
        \\  --foreground: oklch(0.985 0 0);
        \\  --card: oklch(0.205 0 0);
        \\  --card-foreground: oklch(0.985 0 0);
        \\  --popover: oklch(0.205 0 0);
        \\  --popover-foreground: oklch(0.985 0 0);
        \\  --primary: oklch(0.922 0 0);
        \\  --primary-foreground: oklch(0.205 0 0);
        \\  --secondary: oklch(0.269 0 0);
        \\  --secondary-foreground: oklch(0.985 0 0);
        \\  --muted: oklch(0.269 0 0);
        \\  --muted-foreground: oklch(0.708 0 0);
        \\  --accent: oklch(0.269 0 0);
        \\  --accent-foreground: oklch(0.985 0 0);
        \\  --destructive: oklch(0.704 0.191 22.216);
        \\  --border: oklch(1 0 0 / 10%);
        \\  --input: oklch(1 0 0 / 15%);
        \\  --ring: oklch(0.556 0 0);
        \\  --sidebar: oklch(0.205 0 0);
        \\  --sidebar-foreground: oklch(0.985 0 0);
        \\  --sidebar-primary: oklch(0.488 0.243 264.376);
        \\  --sidebar-accent: oklch(0.269 0 0);
        \\  --sidebar-border: oklch(1 0 0 / 10%);
        \\  --radius: 10px;
        \\  --radius-sm: 6px;
        \\  --radius-md: 8px;
        \\  --radius-lg: 10px;
        \\  --radius-xl: 14px;
        \\  --text-xs: 12px;
        \\  --text-sm: 14px;
        \\  --text-base: 16px;
        \\  --text-lg: 18px;
        \\  --space-1: 4px;
        \\  --space-2: 8px;
        \\  --space-3: 12px;
        \\  --space-4: 16px;
        \\  --space-6: 24px;
        \\  --space-8: 32px;
        \\}
    // --- Adwaita bridge: repoint the named colours stock widgets are styled
    //     with at the same web palette (hex equivalents of the tokens above —
    //     @define-color predates the modern colour parser). Elevation ladder
    //     now matches web exactly: content/background #0a0a0a < sidebar/cards/
    //     popovers/dialogs #171717 (web --card/--popover/--sidebar). The indigo
    //     accent is web's --sidebar-primary family (#4f46e5/#818cf8). Lists are
    //     transparent so each region reads as one uniform surface. ---
        \\@define-color window_bg_color #0a0a0a;
        \\@define-color window_fg_color #fafafa;
        \\@define-color view_bg_color #0a0a0a;
        \\@define-color view_fg_color #fafafa;
        \\@define-color card_bg_color #171717;
        \\@define-color card_fg_color #fafafa;
        \\@define-color popover_bg_color #171717;
        \\@define-color popover_fg_color #fafafa;
        \\@define-color dialog_bg_color #171717;
        \\@define-color dialog_fg_color #fafafa;
        \\@define-color headerbar_bg_color #0a0a0a;
        \\@define-color headerbar_fg_color #fafafa;
        \\@define-color sidebar_bg_color #171717;
        \\@define-color sidebar_fg_color #fafafa;
        \\@define-color secondary_sidebar_bg_color #171717;
        \\@define-color accent_bg_color #4f46e5;
        \\@define-color accent_fg_color #ffffff;
        \\@define-color accent_color #818cf8;
        \\@define-color destructive_bg_color #ef4444;
        \\@define-color destructive_fg_color #ffffff;
        \\@define-color destructive_color #f87171;
        \\@define-color exp_border rgba(255,255,255,0.10);
        \\* { font-family: 'Inter', 'Cantarell', 'Adwaita Sans', sans-serif; }
        // Lists never paint their own background — they inherit the region surface.
        \\.navigation-sidebar { background-color: transparent; }
        \\list, list > row { background-color: transparent; }
        // Sidebar pane: one cohesive surface with a single hairline divider. Its
        // header bar drops its own fill/shadow so the band seams disappear.
        \\.exp-sidebar { background-color: @sidebar_bg_color; border-right: 1px solid @exp_border; }
        \\.exp-sidebar-header { background: none; box-shadow: none; }
        // Active project: a clear accent-tinted pill (scoped to the sidebar so the
        // issue list's transient row selection stays untinted).
        \\.exp-sidebar row:selected {
        \\  background-color: alpha(@accent_color, 0.18);
        \\  color: @window_fg_color;
        \\  border-radius: 6px;
        \\}
        \\.exp-sidebar row:selected:hover { background-color: alpha(@accent_color, 0.24); }
    // Label pill — web: border border-border/50 rounded-full px-1.5 py-px
    // text-xs text-muted-foreground.
        \\.exp-chip {
        \\  border: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
        \\  border-radius: 9999px;
        \\  padding: 1px 6px;
        \\  font-size: var(--text-xs);
        \\  color: var(--muted-foreground);
        \\}
    // --- Issue list (issue-list.tsx web parity) ---
    // The list itself is a transparent full-bleed surface; each row's child
    // carries the grid styling so headers/rows control their own backgrounds.
        \\.exp-issue-list { background: transparent; }
        \\.exp-issue-list > row {
        \\  padding: 0;
        \\  border-radius: 0;
        \\  min-height: 0;
        \\  background: none;
        \\}
    // Issue row: h-10 (40px) px-6, text-sm, hover bg-accent/30, hairline
    // border-b border-border/30.
        \\.exp-issue-row {
        \\  min-height: 40px;
        \\  padding: 0 24px;
        \\  font-size: var(--text-sm);
        \\  border-bottom: 1px solid color-mix(in srgb, var(--border) 30%, transparent);
        \\}
        \\.exp-issue-row:hover { background-color: color-mix(in srgb, var(--accent) 30%, transparent); }
    // Identifier column: text-xs text-muted-foreground font-mono.
        \\.exp-ident {
        \\  font-family: monospace;
        \\  font-size: var(--text-xs);
        \\  color: var(--muted-foreground);
        \\}
        \\.exp-text-xs { font-size: var(--text-xs); }
    // Group header: pl-3 pr-6 py-1.5, border-b border-border/50, per-status
    // tinted background (statusHeaderBg map in issue-list.tsx).
        \\.exp-group-header {
        \\  padding: 6px 24px 6px 12px;
        \\  border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
        \\  background-color: rgba(113,113,122,0.08);
        \\}
        \\.exp-group-todo { background-color: rgba(212,212,216,0.08); }
        \\.exp-group-in_progress { background-color: rgba(234,179,8,0.10); }
        \\.exp-group-done { background-color: rgba(34,197,94,0.10); }
        \\.exp-comment {
        \\  background-color: alpha(currentColor, 0.04);
        \\  border-radius: 8px;
        \\  padding: 8px 10px;
        \\}
        \\.exp-icon { font-size: 1.05em; }
        \\.exp-swatch {
        \\  min-width: 26px;
        \\  min-height: 26px;
        \\  padding: 0;
        \\  border-radius: 9999px;
        \\}
        \\.exp-swatch-on {
        \\  border: 2px solid @accent_color;
        \\}
        \\.exp-title-entry {
        \\  font-size: 1.5em;
        \\  font-weight: bold;
        \\  background: none;
        \\  box-shadow: none;
        \\  border: none;
        \\  padding-left: 0;
        \\  min-height: 0;
        \\}
        \\.exp-avatar {
        \\  background-color: alpha(currentColor, 0.14);
        \\  border-radius: 9999px;
        \\  min-width: 20px;
        \\  min-height: 20px;
        \\  font-size: 0.72em;
        \\}
        // --- Control geometry: match the web (shadcn) button scale so GTK
        //     stops rendering libadwaita's oversized default controls. Loaded
        //     at PRIORITY_APPLICATION, these element rules beat Adwaita's
        //     defaults while its more-specific `.linked`/windowcontrols rules
        //     still win. The bare `button` element rule gives every unclassed
        //     button the sm scale (h-8 = 32px); explicit `.exp-btn*` classes
        //     opt into the exact shadcn size variants (see below). ---
        \\button {
        \\  min-height: 32px;
        \\  padding: 0 12px;
        \\  border-radius: var(--radius-md);
        \\  font-size: var(--text-sm);
        \\  font-weight: 500;
        \\}
        \\button.flat {
        \\  background: none;
        \\  box-shadow: none;
        \\  border-color: transparent;
        \\}
        \\button.flat:hover { background-color: alpha(currentColor, 0.08); }
        \\button.suggested-action {
        \\  background-color: @accent_bg_color;
        \\  color: @accent_fg_color;
        \\  box-shadow: none;
        \\}
        \\button.suggested-action:hover { background-color: shade(@accent_bg_color, 1.08); }
        \\button.destructive-action {
        \\  background-color: @destructive_bg_color;
        \\  color: @destructive_fg_color;
        \\  box-shadow: none;
        \\}
        \\button.destructive-action:hover { background-color: shade(@destructive_bg_color, 1.08); }
        // --- exp-btn: shadcn Button sizes (ui/button.tsx) — default h-9 (36px)
        //     px-4, sm h-8 (32px) px-3, xs h-6 (24px) px-2 text-xs, lg h-10
        //     (40px) px-6, icon 36×36, icon-xs 24×24; all rounded-md text-sm
        //     font-medium. GTK has no border-box, so min-height carries the
        //     full control height and vertical padding stays 0 (content
        //     centres within min-height). ---
        \\button.exp-btn { min-height: 36px; padding: 0 16px; }
        \\button.exp-btn-sm { min-height: 32px; padding: 0 12px; }
        \\button.exp-btn-xs { min-height: 24px; padding: 0 8px; font-size: var(--text-xs); }
        \\button.exp-btn-lg { min-height: 40px; padding: 0 24px; }
        \\button.exp-btn-icon { min-height: 36px; min-width: 36px; padding: 0; }
        \\button.exp-btn-icon-xs { min-height: 24px; min-width: 24px; padding: 0; }
        // exp-btn variants — the shadcn Button variant colour recipes on the
        // token layer: default(primary), secondary, outline, ghost, destructive.
        \\button.exp-btn-primary {
        \\  background-color: var(--primary);
        \\  color: var(--primary-foreground);
        \\  box-shadow: none;
        \\}
        \\button.exp-btn-primary:hover { background-color: color-mix(in srgb, var(--primary) 90%, transparent); }
        \\button.exp-btn-secondary {
        \\  background-color: var(--secondary);
        \\  color: var(--secondary-foreground);
        \\  box-shadow: none;
        \\}
        \\button.exp-btn-secondary:hover { background-color: color-mix(in srgb, var(--secondary) 80%, transparent); }
        \\button.exp-btn-outline {
        \\  background-color: color-mix(in srgb, var(--input) 30%, transparent);
        \\  border: 1px solid var(--input);
        \\  color: var(--foreground);
        \\  box-shadow: none;
        \\}
        \\button.exp-btn-outline:hover { background-color: color-mix(in srgb, var(--input) 50%, transparent); }
        \\button.exp-btn-ghost { background: none; border-color: transparent; box-shadow: none; }
        \\button.exp-btn-ghost:hover { background-color: color-mix(in srgb, var(--accent) 50%, transparent); }
        \\button.exp-btn-destructive {
        \\  background-color: color-mix(in srgb, var(--destructive) 60%, transparent);
        \\  color: #ffffff;
        \\  box-shadow: none;
        \\}
        \\button.exp-btn-destructive:hover { background-color: color-mix(in srgb, var(--destructive) 50%, transparent); }
        \\.exp-pill { border-radius: 9999px; padding: 4px 14px; }
        // Filter tabs — web issue-filter-bar: ghost rounded-full h-7 px-3
        // text-xs, muted text; active = bg-accent + foreground text.
        \\button.exp-tab {
        \\  min-height: 28px;
        \\  padding: 0 12px;
        \\  border-radius: 9999px;
        \\  font-size: var(--text-xs);
        \\  color: var(--muted-foreground);
        \\  background: none;
        \\  border-color: transparent;
        \\  box-shadow: none;
        \\}
        \\button.exp-tab:hover {
        \\  color: var(--foreground);
        \\  background-color: color-mix(in srgb, var(--accent) 50%, transparent);
        \\}
        \\button.exp-tab-active, button.exp-tab-active:hover {
        \\  background-color: var(--accent);
        \\  color: var(--foreground);
        \\  font-weight: 500;
        \\}
        // Text inputs: web h-9 (36px) with a hairline border + accent focus ring.
        \\entry, spinbutton, spinbutton > text {
        \\  min-height: 24px;
        \\  padding: 5px 12px;
        \\  border-radius: var(--radius-md);
        \\  font-size: var(--text-sm);
        \\  background-color: color-mix(in srgb, var(--input) 30%, transparent);
        \\  border: 1px solid @exp_border;
        \\  box-shadow: none;
        \\}
        \\entry:focus-within, spinbutton:focus-within {
        \\  border-color: @accent_color;
        \\  box-shadow: none;
        \\}
        // Dropdowns / combo triggers inherit the same compact button shape.
        \\dropdown > button, combobox > box > button.combo {
        \\  min-height: 32px;
        \\  padding: 0 12px;
        \\  border-radius: var(--radius-md);
        \\}
        // Popovers read as the card surface (web --popover, --radius 10px).
        \\popover > contents {
        \\  border-radius: var(--radius);
        \\  padding: 6px;
        \\}
        // Denser list rows (web list items are h-8 with 8px padding).
        \\list > row {
        \\  min-height: 28px;
        \\  border-radius: var(--radius-sm);
        \\}
        \\.card { border-radius: var(--radius); }
        // §3.4 "Remote steering — <name>" strip above a steered terminal tab.
        \\.exp-steer-banner {
        \\  background-color: alpha(#f59e0b, 0.12);
        \\  border-bottom: 1px solid alpha(#f59e0b, 0.35);
        \\  padding: 4px 10px;
        \\  font-size: 13px;
        \\}
    // --- PR diff (web diff-view.tsx parity): bordered rounded card, muted
    //     text-xs mono header, 11px (text-[0.6875rem]) code columns. Line
    //     tints live on GtkTextTags in diffColumn (paragraph-background). ---
        \\.exp-diff-card {
        \\  border: 1px solid var(--border);
        \\  border-radius: var(--radius-md);
        \\}
        \\.exp-diff-header {
        \\  padding: 6px 12px;
        \\  border-bottom: 1px solid var(--border);
        \\  background-color: color-mix(in srgb, var(--muted) 30%, transparent);
        \\}
        \\.exp-diff-file { font-family: monospace; font-size: var(--text-xs); }
        \\textview.exp-diff-code { font-family: monospace; font-size: 11px; }
    );
    const display = gtk.gdk_display_get_default();
    if (display != null)
        gtk.gtk_style_context_add_provider_for_display(display, provider, gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
    gtk.g_object_unref(provider);
}

/// shadcn Button variants/sizes (ui/button.tsx) as CSS-class combos for the
/// exp-btn layer in applyCss. `styleButton` replaces ad-hoc Adwaita classes
/// (`suggested-action`, `pill`, …) at web-parity call sites.
pub const BtnVariant = enum { primary, secondary, outline, ghost, destructive };
pub const BtnSize = enum { default, sm, xs, lg, icon, icon_xs };

pub fn styleButton(btn: gtk.Object, variant: BtnVariant, size: BtnSize) void {
    gtk.gtk_widget_add_css_class(btn, switch (size) {
        .default => "exp-btn",
        .sm => "exp-btn-sm",
        .xs => "exp-btn-xs",
        .lg => "exp-btn-lg",
        .icon => "exp-btn-icon",
        .icon_xs => "exp-btn-icon-xs",
    });
    gtk.gtk_widget_add_css_class(btn, switch (variant) {
        .primary => "exp-btn-primary",
        .secondary => "exp-btn-secondary",
        .outline => "exp-btn-outline",
        .ghost => "exp-btn-ghost",
        .destructive => "exp-btn-destructive",
    });
}

/// Create a labelled button pre-styled with the exp-btn classes.
pub fn button(label: [*:0]const u8, variant: BtnVariant, size: BtnSize) gtk.Object {
    const btn = gtk.gtk_button_new_with_label(label);
    styleButton(btn, variant, size);
    return btn;
}

/// A label rendering a coloured glyph (status/priority icon). The markup is
/// fixed-size (glyph ≤ 4 bytes + a #rrggbb colour), so a stack buffer suffices.
pub fn iconLabel(opt: format.Option) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    var buf: [96]u8 = undefined;
    const markup = std.fmt.bufPrintZ(&buf, "<span foreground='{s}'>{s}</span>", .{ opt.color, opt.glyph }) catch return lbl;
    gtk.gtk_label_set_markup(lbl, markup.ptr);
    gtk.gtk_widget_add_css_class(lbl, "exp-icon");
    return lbl;
}

pub fn statusIcon(value: []const u8) gtk.Object {
    return iconLabel(format.status(value));
}

pub fn priorityIcon(value: []const u8) gtk.Object {
    return iconLabel(format.priority(value));
}

/// A small coloured dot (used for label glyphs on dense list rows).
pub fn dot(color: []const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    var buf: [64]u8 = undefined;
    const markup = std.fmt.bufPrintZ(&buf, "<span foreground='{s}'>●</span>", .{color}) catch return lbl;
    gtk.gtk_label_set_markup(lbl, markup.ptr);
    return lbl;
}

/// A rounded label chip: coloured dot + name. `name` need not be NUL-terminated.
pub fn chip(arena: std.mem.Allocator, name: []const u8, color: []const u8) gtk.Object {
    const box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
    gtk.gtk_widget_add_css_class(box, "exp-chip");
    gtk.gtk_box_append(box, dot(color));
    const lbl = gtk.gtk_label_new(null);
    if (arena.dupeZ(u8, name)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_box_append(box, lbl);
    return box;
}

/// A 2×10 grid of round colour swatches (the shared 20-colour palette). Each
/// swatch stashes its colour as "exp-color" data and calls `handler(btn, data)`
/// on click; the swatch matching `selected_color` is ringed and written to
/// `selected_out`. Reused by create-project and the labels settings section.
pub fn swatchGrid(data: gtk.gpointer, handler: gtk.GCallback, selected_color: []const u8, selected_out: *gtk.Object) gtk.Object {
    const grid = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 4);
    var roww: gtk.Object = null;
    for (format.label_colors, 0..) |color, i| {
        if (i % 10 == 0) {
            roww = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
            gtk.gtk_box_append(grid, roww);
        }
        const sw = gtk.gtk_button_new();
        gtk.gtk_widget_add_css_class(sw, "flat");
        gtk.gtk_widget_add_css_class(sw, "exp-swatch");
        const lbl = gtk.gtk_label_new(null);
        var buf: [64]u8 = undefined;
        if (std.fmt.bufPrintZ(&buf, "<span size='large' foreground='{s}'>\u{25CF}</span>", .{color})) |z| {
            gtk.gtk_label_set_markup(lbl, z.ptr);
        } else |_| {}
        gtk.gtk_button_set_child(sw, lbl);
        // The palette entries are comptime literals, so their pointers are valid
        // for the program's lifetime — no destroy notify needed.
        gtk.g_object_set_data_full(sw, "exp-color", @ptrCast(@constCast(color.ptr)), null);
        if (std.mem.eql(u8, color, selected_color)) {
            gtk.gtk_widget_add_css_class(sw, "exp-swatch-on");
            selected_out.* = sw;
        }
        _ = gtk.g_signal_connect_data(sw, "clicked", handler, data, null, 0);
        gtk.gtk_box_append(roww, sw);
    }
    return grid;
}

/// A circular initial avatar for a user (first codepoint, uppercased if ASCII),
/// with the full name as a tooltip. `name` must be non-empty.
pub fn avatar(arena: std.mem.Allocator, name: []const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    gtk.gtk_widget_add_css_class(lbl, "exp-avatar");
    const n = std.unicode.utf8ByteSequenceLength(name[0]) catch 1;
    var buf: [5]u8 = undefined;
    const first = name[0..@min(n, name.len)];
    @memcpy(buf[0..first.len], first);
    if (first.len == 1 and buf[0] >= 'a' and buf[0] <= 'z') buf[0] -= 32;
    buf[first.len] = 0;
    gtk.gtk_label_set_text(lbl, @ptrCast(&buf));
    if (arena.dupeZ(u8, name)) |z| gtk.gtk_widget_set_tooltip_text(lbl, z.ptr) else |_| {}
    return lbl;
}

/// A start-aligned section title ("Description", "Comments", …).
pub fn sectionTitle(text: [*:0]const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(text);
    gtk.gtk_widget_add_css_class(lbl, "title-4");
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_widget_set_margin_top(lbl, 8);
    return lbl;
}

/// A horizontal row: coloured icon + plain text label (used inside picker
/// popovers and the detail properties row).
pub fn iconTextRow(arena: std.mem.Allocator, icon: gtk.Object, text: []const u8) gtk.Object {
    const box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_box_append(box, icon);
    const lbl = gtk.gtk_label_new(null);
    if (arena.dupeZ(u8, text)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(box, lbl);
    return box;
}
