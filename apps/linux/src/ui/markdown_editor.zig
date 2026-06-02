//! WYSIWYG-as-possible markdown editor.
//!
//! A GtkTextView whose buffer holds **markdown text plus inline image anchors**.
//! Text is live-styled (headings large, bold/italic/strike/code rendered, markers
//! dimmed). Images are real inline widgets (GtkTextChildAnchor → GtkPicture); the
//! markdown for each image is stored on its anchor. `getText` walks the buffer,
//! emitting characters and each anchor's stored `![alt](url)` — so the GFM
//! contract round-trips even with images in the middle of the text.
//!
//! Image upload (file → multipart POST) and download (attachment → GtkPicture)
//! both run on a worker thread and post their result back via g_idle_add, so the
//! UI never blocks. The same widget renders read-only (detail pane) by hiding the
//! toolbar and disabling editing — giving the detail view the same markdown
//! styling and async inline images as the editor.

const std = @import("std");
const gtk = @import("gtk.zig");
const http = @import("../core/api/http.zig");

pub const MarkdownEditor = struct {
    gpa: std.mem.Allocator,
    container: gtk.Object, // vbox: toolbar + scrolled(view) — embed this
    toolbar: gtk.Object = null,
    scrolled: gtk.Object = null,
    view: gtk.Object,
    buffer: gtk.Object,
    image_button: gtk.Object = null,
    // Issue context for image upload/fetch (borrowed; valid for the dialog).
    base_url: ?[]const u8 = null,
    token: ?[]const u8 = null,
    issue_id: ?[]const u8 = null,

    pub fn create(gpa: std.mem.Allocator) ?*MarkdownEditor {
        const self = gpa.create(MarkdownEditor) catch return null;
        self.gpa = gpa;
        self.base_url = null;
        self.token = null;
        self.issue_id = null;

        const view = gtk.gtk_text_view_new();
        gtk.gtk_text_view_set_wrap_mode(view, gtk.WRAP_WORD_CHAR);
        gtk.gtk_text_view_set_left_margin(view, 8);
        gtk.gtk_text_view_set_top_margin(view, 8);
        const buffer = gtk.gtk_text_view_get_buffer(view);
        self.view = view;
        self.buffer = buffer;

        _ = gtk.gtk_text_buffer_create_tag(buffer, "h1", "weight", @as(c_int, 700), "scale", @as(f64, 1.6), @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "h2", "weight", @as(c_int, 700), "scale", @as(f64, 1.3), @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "h3", "weight", @as(c_int, 700), "scale", @as(f64, 1.15), @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "bold", "weight", @as(c_int, 700), @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "italic", "style", @as(c_int, 2), @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "strike", "strikethrough", @as(c_int, 1), @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "code", "family", "monospace", @as(?*anyopaque, null));
        _ = gtk.gtk_text_buffer_create_tag(buffer, "marker", "foreground", "#888888", @as(?*anyopaque, null));

        _ = gtk.g_signal_connect_data(buffer, "changed", @ptrCast(&onChanged), self, null, 0);

        const toolbar = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 2);
        gtk.gtk_widget_add_css_class(toolbar, "toolbar");
        self.toolbar = toolbar;
        addButton(self, toolbar, "B", &onBold);
        addButton(self, toolbar, "I", &onItalic);
        addButton(self, toolbar, "S", &onStrike);
        addButton(self, toolbar, "</>", &onCode);
        addButton(self, toolbar, "H1", &onH1);
        addButton(self, toolbar, "H2", &onH2);
        addButton(self, toolbar, "H3", &onH3);
        addButton(self, toolbar, "• List", &onBullet);
        addButton(self, toolbar, "1. List", &onNumber);
        addButton(self, toolbar, "❝", &onQuote);

        const image_button = gtk.gtk_button_new_with_label("🖼 Image");
        gtk.gtk_widget_add_css_class(image_button, "flat");
        gtk.gtk_widget_set_visible(image_button, 0);
        _ = gtk.g_signal_connect_data(image_button, "clicked", @ptrCast(&onImage), self, null, 0);
        gtk.gtk_box_append(toolbar, image_button);
        self.image_button = image_button;

        const scrolled = gtk.gtk_scrolled_window_new();
        gtk.gtk_widget_set_vexpand(scrolled, 1);
        gtk.gtk_widget_add_css_class(scrolled, "card");
        gtk.gtk_scrolled_window_set_child(scrolled, view);
        self.scrolled = scrolled;

        const container = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 6);
        gtk.gtk_box_append(container, toolbar);
        gtk.gtk_box_append(container, scrolled);
        self.container = container;

        return self;
    }

    /// Walk the buffer, emitting text and each image anchor's stored markdown.
    pub fn getText(self: *MarkdownEditor, arena: std.mem.Allocator) ![:0]u8 {
        var out: std.ArrayList(u8) = .empty;
        defer out.deinit(arena);
        var iter: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_start_iter(self.buffer, @ptrCast(&iter));
        while (gtk.gtk_text_iter_is_end(@ptrCast(&iter)) == 0) {
            const anchor = gtk.gtk_text_iter_get_child_anchor(@ptrCast(&iter));
            if (anchor != null) {
                if (gtk.g_object_get_data(anchor, "exp-md")) |m| {
                    try out.appendSlice(arena, std.mem.span(@as([*:0]const u8, @ptrCast(m))));
                }
            } else {
                const cp = gtk.gtk_text_iter_get_char(@ptrCast(&iter));
                if (cp != 0) {
                    var b: [4]u8 = undefined;
                    const n = std.unicode.utf8Encode(@intCast(cp), &b) catch 0;
                    if (n > 0) try out.appendSlice(arena, b[0..n]);
                }
            }
            _ = gtk.gtk_text_iter_forward_char(@ptrCast(&iter));
        }
        return arena.dupeZ(u8, out.items);
    }

    /// Replace the content with parsed markdown: text segments + inline image
    /// anchors (each `![alt](/api/attachments/…)` is fetched and rendered).
    pub fn setText(self: *MarkdownEditor, md: []const u8) void {
        gtk.gtk_text_buffer_set_text(self.buffer, "", 0);
        var i: usize = 0;
        while (i < md.len) {
            const img = std.mem.indexOfPos(u8, md, i, "![") orelse {
                self.insertPlainAtEnd(md[i..]);
                break;
            };
            const alt_close = std.mem.indexOfPos(u8, md, img + 2, "](");
            const url_close = if (alt_close) |ac| std.mem.indexOfPos(u8, md, ac + 2, ")") else null;
            if (alt_close == null or url_close == null) {
                self.insertPlainAtEnd(md[i..]);
                break;
            }
            if (img > i) self.insertPlainAtEnd(md[i..img]);
            self.insertImageAtEnd(md[img + 2 .. alt_close.?], md[alt_close.? + 2 .. url_close.?]);
            i = url_close.? + 1;
        }
        self.restyle();
    }

    pub fn setIssueContext(self: *MarkdownEditor, base_url: ?[]const u8, token: ?[]const u8, issue_id: ?[]const u8) void {
        self.base_url = base_url;
        self.token = token;
        self.issue_id = issue_id;
        gtk.gtk_widget_set_visible(self.image_button, if (issue_id != null) 1 else 0);
    }

    /// Render-only mode: no toolbar, no editing/cursor. Image context still
    /// drives async inline-image downloads, so the detail pane gets the same
    /// markdown styling and progressive images as the editor.
    pub fn setReadOnly(self: *MarkdownEditor, read_only: bool) void {
        gtk.gtk_text_view_set_editable(self.view, if (read_only) 0 else 1);
        gtk.gtk_text_view_set_cursor_visible(self.view, if (read_only) 0 else 1);
        if (self.toolbar != null) gtk.gtk_widget_set_visible(self.toolbar, if (read_only) 0 else 1);
        if (self.scrolled != null and read_only) {
            // Size to content (capped) instead of grabbing all vertical space, so
            // the description sits inline within the scrollable detail pane.
            gtk.gtk_widget_set_vexpand(self.scrolled, 0);
            gtk.gtk_scrolled_window_set_propagate_natural_height(self.scrolled, 1);
            gtk.gtk_scrolled_window_set_max_content_height(self.scrolled, 600);
            gtk.gtk_widget_remove_css_class(self.scrolled, "card");
        }
    }

    fn insertPlainAtEnd(self: *MarkdownEditor, text: []const u8) void {
        if (text.len == 0) return;
        var it: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_end_iter(self.buffer, @ptrCast(&it));
        gtk.gtk_text_buffer_insert(self.buffer, @ptrCast(&it), text.ptr, @intCast(text.len));
    }

    /// Insert an inline image anchor at the end (on load). Attachment images get
    /// an empty GtkPicture placeholder + an async download that fills it in;
    /// non-attachment urls show their alt text.
    fn insertImageAtEnd(self: *MarkdownEditor, alt: []const u8, url: []const u8) void {
        var it: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_end_iter(self.buffer, @ptrCast(&it));
        const anchor = gtk.gtk_text_buffer_create_child_anchor(self.buffer, @ptrCast(&it));
        storeAnchorMd(self.gpa, anchor, alt, url);
        if (std.mem.indexOf(u8, url, "/api/attachments/") != null) {
            const pic = gtk.gtk_picture_new();
            gtk.gtk_picture_set_can_shrink(pic, 1);
            gtk.gtk_picture_set_content_fit(pic, gtk.CONTENT_FIT_SCALE_DOWN);
            gtk.gtk_widget_set_halign(pic, gtk.ALIGN_START);
            gtk.gtk_widget_set_size_request(pic, 200, 240);
            gtk.gtk_text_view_add_child_at_anchor(self.view, pic, anchor);
            startDownload(self, pic, url);
        } else {
            const z = self.gpa.dupeZ(u8, if (alt.len > 0) alt else url) catch return;
            defer self.gpa.free(z);
            gtk.gtk_text_view_add_child_at_anchor(self.view, gtk.gtk_label_new(z.ptr), anchor);
        }
    }

    fn applyTag(self: *MarkdownEditor, name: [*:0]const u8, co_start: usize, co_end: usize) void {
        var a: [128]u8 align(8) = undefined;
        var b: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_iter_at_offset(self.buffer, @ptrCast(&a), @intCast(co_start));
        gtk.gtk_text_buffer_get_iter_at_offset(self.buffer, @ptrCast(&b), @intCast(co_end));
        gtk.gtk_text_buffer_apply_tag_by_name(self.buffer, name, @ptrCast(&a), @ptrCast(&b));
    }

    /// Re-apply formatting tags from the markdown text. Uses get_slice so the
    /// char offsets stay aligned with the buffer when image anchors are present.
    fn restyle(self: *MarkdownEditor) void {
        var s: [128]u8 align(8) = undefined;
        var e: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_bounds(self.buffer, @ptrCast(&s), @ptrCast(&e));
        gtk.gtk_text_buffer_remove_all_tags(self.buffer, @ptrCast(&s), @ptrCast(&e));
        const text_c = gtk.gtk_text_buffer_get_slice(self.buffer, @ptrCast(&s), @ptrCast(&e), 1) orelse return;
        defer gtk.g_free(@ptrCast(text_c));
        const text = std.mem.span(text_c);

        var i: usize = 0;
        var co: usize = 0;
        var at_line_start = true;

        while (i < text.len) {
            if (at_line_start and text[i] == '#') {
                var h: usize = 0;
                while (i + h < text.len and text[i + h] == '#' and h < 3) : (h += 1) {}
                if (i + h < text.len and text[i + h] == ' ') {
                    const line_end = std.mem.indexOfScalarPos(u8, text, i, '\n') orelse text.len;
                    const line_chars = utf8Count(text[i..line_end]);
                    const tag: [*:0]const u8 = if (h == 1) "h1" else if (h == 2) "h2" else "h3";
                    self.applyTag(tag, co, co + line_chars);
                    self.applyTag("marker", co, co + h + 1);
                    co += line_chars;
                    i = line_end;
                    at_line_start = false;
                    continue;
                }
            }

            const c = text[i];
            if (c == '\n') {
                at_line_start = true;
                i += 1;
                co += 1;
                continue;
            }
            at_line_start = false;

            if (c == '*' and i + 1 < text.len and text[i + 1] == '*') {
                if (findMarker(text, i + 2, "**")) |close| {
                    const inner = utf8Count(text[i + 2 .. close]);
                    self.applyTag("bold", co + 2, co + 2 + inner);
                    self.applyTag("marker", co, co + 2);
                    self.applyTag("marker", co + 2 + inner, co + 4 + inner);
                    co += 4 + inner;
                    i = close + 2;
                    continue;
                }
            }
            if (c == '*' and (i + 1 >= text.len or text[i + 1] != '*')) {
                if (findChar(text, i + 1, '*')) |close| {
                    const inner = utf8Count(text[i + 1 .. close]);
                    self.applyTag("italic", co + 1, co + 1 + inner);
                    self.applyTag("marker", co, co + 1);
                    self.applyTag("marker", co + 1 + inner, co + 2 + inner);
                    co += 2 + inner;
                    i = close + 1;
                    continue;
                }
            }
            if (c == '~' and i + 1 < text.len and text[i + 1] == '~') {
                if (findMarker(text, i + 2, "~~")) |close| {
                    const inner = utf8Count(text[i + 2 .. close]);
                    self.applyTag("strike", co + 2, co + 2 + inner);
                    self.applyTag("marker", co, co + 2);
                    self.applyTag("marker", co + 2 + inner, co + 4 + inner);
                    co += 4 + inner;
                    i = close + 2;
                    continue;
                }
            }
            if (c == '`') {
                if (findChar(text, i + 1, '`')) |close| {
                    const inner = utf8Count(text[i + 1 .. close]);
                    self.applyTag("code", co, co + 2 + inner);
                    co += 2 + inner;
                    i = close + 1;
                    continue;
                }
            }

            i += utf8Len(c);
            co += 1;
        }
    }

    fn wrapInline(self: *MarkdownEditor, marker: []const u8) void {
        var s: [128]u8 align(8) = undefined;
        var e: [128]u8 align(8) = undefined;
        if (gtk.gtk_text_buffer_get_selection_bounds(self.buffer, @ptrCast(&s), @ptrCast(&e)) != 0) {
            const sel_c = gtk.gtk_text_buffer_get_text(self.buffer, @ptrCast(&s), @ptrCast(&e), 0) orelse return;
            defer gtk.g_free(@ptrCast(sel_c));
            const sel = std.mem.span(sel_c);
            const wrapped = std.fmt.allocPrint(self.gpa, "{s}{s}{s}", .{ marker, sel, marker }) catch return;
            defer self.gpa.free(wrapped);
            gtk.gtk_text_buffer_delete(self.buffer, @ptrCast(&s), @ptrCast(&e));
            gtk.gtk_text_buffer_insert(self.buffer, @ptrCast(&s), wrapped.ptr, @intCast(wrapped.len));
        } else {
            const doubled = std.fmt.allocPrint(self.gpa, "{s}{s}", .{ marker, marker }) catch return;
            defer self.gpa.free(doubled);
            gtk.gtk_text_buffer_insert_at_cursor(self.buffer, doubled.ptr, @intCast(doubled.len));
        }
    }

    fn prefixLine(self: *MarkdownEditor, prefix: []const u8) void {
        var iter: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_iter_at_mark(self.buffer, @ptrCast(&iter), gtk.gtk_text_buffer_get_insert(self.buffer));
        gtk.gtk_text_iter_set_line_offset(@ptrCast(&iter), 0);
        gtk.gtk_text_buffer_insert(self.buffer, @ptrCast(&iter), prefix.ptr, @intCast(prefix.len));
    }
};

/// Decode image bytes into a scaled-down inline GtkPicture (or null on failure).
fn pictureFromBytes(data: []const u8) ?gtk.Object {
    if (data.len == 0) return null;
    const bytes = gtk.g_bytes_new(data.ptr, data.len);
    defer gtk.g_bytes_unref(bytes);
    const texture = gtk.gdk_texture_new_from_bytes(bytes, null);
    if (texture == null) return null;
    defer gtk.g_object_unref(texture);
    const pic = gtk.gtk_picture_new_for_paintable(texture);
    gtk.gtk_picture_set_can_shrink(pic, 1);
    gtk.gtk_picture_set_content_fit(pic, gtk.CONTENT_FIT_SCALE_DOWN);
    gtk.gtk_widget_set_halign(pic, gtk.ALIGN_START);
    gtk.gtk_widget_set_size_request(pic, -1, 240);
    return pic;
}

fn addButton(
    self: *MarkdownEditor,
    toolbar: gtk.Object,
    label: [*:0]const u8,
    handler: *const fn (gtk.Object, gtk.gpointer) callconv(.c) void,
) void {
    const btn = gtk.gtk_button_new_with_label(label);
    gtk.gtk_widget_add_css_class(btn, "flat");
    _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(handler), self, null, 0);
    gtk.gtk_box_append(toolbar, btn);
}

fn editorOf(data: gtk.gpointer) *MarkdownEditor {
    return @ptrCast(@alignCast(data));
}

fn onChanged(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    editorOf(data).restyle();
}

fn onBold(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).wrapInline("**");
}
fn onItalic(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).wrapInline("*");
}
fn onStrike(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).wrapInline("~~");
}
fn onCode(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).wrapInline("`");
}
fn onH1(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).prefixLine("# ");
}
fn onH2(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).prefixLine("## ");
}
fn onH3(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).prefixLine("### ");
}
fn onBullet(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).prefixLine("- ");
}
fn onNumber(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).prefixLine("1. ");
}
fn onQuote(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    editorOf(d).prefixLine("> ");
}

fn onImage(_: gtk.Object, d: gtk.gpointer) callconv(.c) void {
    const self = editorOf(d);
    if (self.issue_id == null) return;
    const dialog = gtk.gtk_file_dialog_new();
    gtk.gtk_file_dialog_open(dialog, gtk.gtk_widget_get_root(self.view), null, &onFileChosen, self);
}

fn onFileChosen(source: gtk.Object, result: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    defer gtk.g_object_unref(source);
    const self = editorOf(data);
    const file = gtk.gtk_file_dialog_open_finish(source, result, null);
    if (file == null) return;
    defer gtk.g_object_unref(file);
    const path_c = gtk.g_file_get_path(file) orelse return;
    defer gtk.g_free(@ptrCast(path_c));
    startUpload(self, std.mem.span(path_c));
}

const boundary = "----ExponentialBoundary7MA4YWxkTrZu0gW";

fn storeAnchorMd(gpa: std.mem.Allocator, anchor: gtk.Object, alt: []const u8, url: []const u8) void {
    if (std.fmt.allocPrintSentinel(gpa, "![{s}]({s})", .{ alt, url }, 0)) |mdz| {
        defer gpa.free(mdz);
        gtk.g_object_set_data_full(anchor, "exp-md", @ptrCast(gtk.g_strdup(mdz.ptr)), @ptrCast(&gtk.g_free));
    } else |_| {}
}

fn insertImageInto(gpa: std.mem.Allocator, buffer: gtk.Object, view: gtk.Object, alt: []const u8, url: []const u8, bytes: []const u8) void {
    var it: [128]u8 align(8) = undefined;
    gtk.gtk_text_buffer_get_iter_at_mark(buffer, @ptrCast(&it), gtk.gtk_text_buffer_get_insert(buffer));
    const anchor = gtk.gtk_text_buffer_create_child_anchor(buffer, @ptrCast(&it));
    storeAnchorMd(gpa, anchor, alt, url);
    gtk.gtk_text_view_add_child_at_anchor(view, pictureFromBytes(bytes) orelse gtk.gtk_label_new("🖼"), anchor);
}

// --- async image upload (worker thread + g_idle_add; buffer/view are refed so
//     a closed dialog mid-upload is safe) ---

const UploadJob = struct {
    gpa: std.mem.Allocator,
    buffer: gtk.Object,
    view: gtk.Object,
    base_url: []u8,
    token: ?[]u8,
    issue_id: []u8,
    fname: []u8,
    data: []u8 = &.{},
    url: ?[]u8 = null,
};

fn startUpload(self: *MarkdownEditor, path: []const u8) void {
    const gpa = self.gpa;
    const base = self.base_url orelse return;
    const issue = self.issue_id orelse return;
    const io = std.Io.Threaded.global_single_threaded.io();
    const data = std.Io.Dir.cwd().readFileAlloc(io, path, gpa, .unlimited) catch return;

    const job = buildUploadJob(self, base, issue, path) orelse {
        gpa.free(data);
        return;
    };
    job.data = data;
    _ = gtk.g_object_ref(job.buffer);
    _ = gtk.g_object_ref(job.view);

    const th = std.Thread.spawn(.{}, uploadWorker, .{job}) catch {
        uploadWorker(job); // fallback: run inline (still posts the result via g_idle)
        return;
    };
    th.detach();
}

fn buildUploadJob(self: *MarkdownEditor, base: []const u8, issue: []const u8, path: []const u8) ?*UploadJob {
    const gpa = self.gpa;
    const job = gpa.create(UploadJob) catch return null;
    job.gpa = gpa;
    job.buffer = self.buffer;
    job.view = self.view;
    job.url = null;
    job.data = &.{};
    job.base_url = gpa.dupe(u8, base) catch {
        gpa.destroy(job);
        return null;
    };
    job.issue_id = gpa.dupe(u8, issue) catch {
        gpa.free(job.base_url);
        gpa.destroy(job);
        return null;
    };
    job.fname = gpa.dupe(u8, std.fs.path.basename(path)) catch {
        gpa.free(job.base_url);
        gpa.free(job.issue_id);
        gpa.destroy(job);
        return null;
    };
    job.token = if (self.token) |t| (gpa.dupe(u8, t) catch {
        gpa.free(job.base_url);
        gpa.free(job.issue_id);
        gpa.free(job.fname);
        gpa.destroy(job);
        return null;
    }) else null;
    return job;
}

fn uploadWorker(job: *UploadJob) void {
    const gpa = job.gpa;
    defer _ = gtk.g_idle_add(@ptrCast(&onUploadDone), job);

    const ct = guessContentType(job.fname);
    const header = std.fmt.allocPrint(
        gpa,
        "--{s}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{s}\"\r\nContent-Type: {s}\r\n\r\n",
        .{ boundary, job.fname, ct },
    ) catch return;
    defer gpa.free(header);
    const footer = "\r\n--" ++ boundary ++ "--\r\n";
    const body = gpa.alloc(u8, header.len + job.data.len + footer.len) catch return;
    defer gpa.free(body);
    @memcpy(body[0..header.len], header);
    @memcpy(body[header.len..][0..job.data.len], job.data);
    @memcpy(body[header.len + job.data.len ..], footer);

    const url = std.fmt.allocPrintSentinel(gpa, "{s}/api/issues/{s}/images", .{ std.mem.trimEnd(u8, job.base_url, "/"), job.issue_id }, 0) catch return;
    defer gpa.free(url);
    var resp = http.postMultipart(gpa, url, job.token, body, "multipart/form-data; boundary=" ++ boundary, 60) catch return;
    defer resp.deinit();
    if (resp.status < 200 or resp.status >= 300) return;

    const parsed = std.json.parseFromSlice(std.json.Value, gpa, resp.body, .{}) catch return;
    defer parsed.deinit();
    switch (parsed.value) {
        .object => |obj| {
            if (obj.get("url")) |u| switch (u) {
                .string => |s| job.url = gpa.dupe(u8, s) catch null,
                else => {},
            };
        },
        else => {},
    }
}

fn onUploadDone(data: gtk.gpointer) callconv(.c) c_int {
    const job: *UploadJob = @ptrCast(@alignCast(data));
    if (job.url) |u| insertImageInto(job.gpa, job.buffer, job.view, job.fname, u, job.data);
    gtk.g_object_unref(job.buffer);
    gtk.g_object_unref(job.view);
    const gpa = job.gpa;
    gpa.free(job.base_url);
    gpa.free(job.issue_id);
    gpa.free(job.fname);
    gpa.free(job.data);
    if (job.token) |t| gpa.free(t);
    if (job.url) |u| gpa.free(u);
    gpa.destroy(job);
    return 0;
}

// --- async image download (fills an existing placeholder picture; refed) ---

const DownloadJob = struct {
    gpa: std.mem.Allocator,
    picture: gtk.Object,
    url: [:0]u8,
    token: ?[]u8,
    data: ?[]u8 = null,
};

fn startDownload(self: *MarkdownEditor, picture: gtk.Object, url: []const u8) void {
    const gpa = self.gpa;
    const base = self.base_url orelse return;
    const abs = std.fmt.allocPrintSentinel(gpa, "{s}{s}", .{ std.mem.trimEnd(u8, base, "/"), url }, 0) catch return;
    const job = gpa.create(DownloadJob) catch {
        gpa.free(abs);
        return;
    };
    job.gpa = gpa;
    job.picture = picture;
    job.url = abs;
    job.token = if (self.token) |t| (gpa.dupe(u8, t) catch null) else null;
    job.data = null;
    _ = gtk.g_object_ref(picture);

    const th = std.Thread.spawn(.{}, downloadWorker, .{job}) catch {
        downloadWorker(job);
        return;
    };
    th.detach();
}

fn downloadWorker(job: *DownloadJob) void {
    defer _ = gtk.g_idle_add(@ptrCast(&onDownloadDone), job);
    var resp = http.get(job.gpa, job.url, job.token, 20, null) catch return;
    defer resp.deinit();
    if (resp.status >= 200 and resp.status < 300 and resp.body.len > 0) {
        job.data = job.gpa.dupe(u8, resp.body) catch null;
    }
}

fn onDownloadDone(data: gtk.gpointer) callconv(.c) c_int {
    const job: *DownloadJob = @ptrCast(@alignCast(data));
    if (job.data) |d| {
        const bytes = gtk.g_bytes_new(d.ptr, d.len);
        defer gtk.g_bytes_unref(bytes);
        const texture = gtk.gdk_texture_new_from_bytes(bytes, null);
        if (texture != null) {
            gtk.gtk_picture_set_paintable(job.picture, texture);
            gtk.g_object_unref(texture);
        }
    }
    gtk.g_object_unref(job.picture);
    const gpa = job.gpa;
    gpa.free(job.url);
    if (job.token) |t| gpa.free(t);
    if (job.data) |d| gpa.free(d);
    gpa.destroy(job);
    return 0;
}

fn guessContentType(name: []const u8) []const u8 {
    if (std.ascii.endsWithIgnoreCase(name, ".png")) return "image/png";
    if (std.ascii.endsWithIgnoreCase(name, ".jpg") or std.ascii.endsWithIgnoreCase(name, ".jpeg")) return "image/jpeg";
    if (std.ascii.endsWithIgnoreCase(name, ".gif")) return "image/gif";
    if (std.ascii.endsWithIgnoreCase(name, ".webp")) return "image/webp";
    return "application/octet-stream";
}

// --- scan helpers (char-offset aware) ---

fn utf8Len(c: u8) usize {
    if (c < 0x80) return 1;
    if (c >= 0xF0) return 4;
    if (c >= 0xE0) return 3;
    if (c >= 0xC0) return 2;
    return 1;
}

fn utf8Count(slice: []const u8) usize {
    var n: usize = 0;
    for (slice) |b| {
        if (b & 0xC0 != 0x80) n += 1;
    }
    return n;
}

fn findMarker(text: []const u8, from: usize, marker: []const u8) ?usize {
    if (from > text.len) return null;
    const nl = std.mem.indexOfScalarPos(u8, text, from, '\n') orelse text.len;
    const idx = std.mem.indexOfPos(u8, text, from, marker) orelse return null;
    return if (idx < nl) idx else null;
}

fn findChar(text: []const u8, from: usize, ch: u8) ?usize {
    if (from >= text.len) return null;
    const nl = std.mem.indexOfScalarPos(u8, text, from, '\n') orelse text.len;
    const idx = std.mem.indexOfScalarPos(u8, text, from, ch) orelse return null;
    return if (idx < nl) idx else null;
}
