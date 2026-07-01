//! Authenticated-direct feedback report: file the annotated preview screenshot
//! as an issue created by the logged-in developer (no `expw_` widget key, no
//! synthetic user — that's the web embeddable widget's path, not the native
//! overlay's). The server rejects images on create, so the sequence is strictly
//! create → upload → update (mirrors WS2/macOS `MacFeedbackReporter` and the
//! web report path):
//!
//!   1. issues.create  { projectId, title, description:<text>, status:"backlog" }
//!                       → { issue:{ id, identifier } }
//!   2. POST /api/issues/{id}/images   (multipart flattened PNG/JPEG)
//!                       → { url:"/api/attachments/{id}", width, height }
//!   3. issues.update  description + "\n\n![screenshot](/api/attachments/{id})"
//!      (the server runs canonicalizeMarkdownImageUrls on save)
//!
//! Target project = mirror.feedbackProjectId ?? previewedProjectId; the
//! workspace is derived server-side via assertWorkspaceMember. Graceful
//! degradation: if the upload fails we keep the text issue and toast
//! "screenshot upload failed" instead of aborting.
//!
//! Threading: the whole 3-call chain runs on a detached worker thread (network),
//! posting its result toast back to the GTK main loop via g_idle_add. The
//! flattened image bytes are produced on the main thread (Cairo/GdkPixbuf) by
//! the annotation overlay and handed in here already-encoded.

const std = @import("std");
const gtk = @import("../gtk.zig");
const http = @import("../../core/api/http.zig");
const trpc = @import("../../core/api/trpc.zig");

// Same multipart boundary idiom as markdown_editor.zig's image upload.
const boundary = "----ExponentialBoundary7MA4YWxkTrZu0gW";

/// Everything the worker needs; all slices are gpa-owned copies so the caller's
/// arena/UI can go away mid-flight. `image` is the already-flattened encoded
/// screenshot (PNG/JPEG); null files a text-only issue.
pub const Report = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    project_id: []u8,
    title: []u8,
    description: []u8,
    image: ?[]u8 = null,
    image_format: []u8, // "png"/"jpeg" — drives the upload filename + content type
    // Optional: a window/widget to anchor a result toast under. Opaque so this
    // module needn't import the AppState; null disables the toast.
    toast_parent: gtk.Object = null,
};

/// Build a Report from borrowed inputs (duping everything) and fire it on a
/// detached worker. Returns false if the initial allocation failed (caller can
/// surface that); true once the worker is launched (its result is async).
pub fn fire(
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    project_id: []const u8,
    title: []const u8,
    description: []const u8,
    image: ?[]const u8,
    image_format: []const u8,
    toast_parent: gtk.Object,
) bool {
    // Dupe everything into an arena first; on any OOM the arena cleans up and we
    // bail without a partially-owned Report. Only on full success do we move the
    // owned slices into the gpa-backed Report the worker frees.
    const instance_d = gpa.dupe(u8, instance) catch return false;
    const project_d = gpa.dupe(u8, project_id) catch {
        gpa.free(instance_d);
        return false;
    };
    const title_d = gpa.dupe(u8, title) catch {
        gpa.free(instance_d);
        gpa.free(project_d);
        return false;
    };
    const desc_d = gpa.dupe(u8, description) catch {
        gpa.free(instance_d);
        gpa.free(project_d);
        gpa.free(title_d);
        return false;
    };
    const fmt_d = gpa.dupe(u8, image_format) catch {
        gpa.free(instance_d);
        gpa.free(project_d);
        gpa.free(title_d);
        gpa.free(desc_d);
        return false;
    };

    const r = gpa.create(Report) catch {
        gpa.free(instance_d);
        gpa.free(project_d);
        gpa.free(title_d);
        gpa.free(desc_d);
        gpa.free(fmt_d);
        return false;
    };
    r.* = .{
        .gpa = gpa,
        .instance = instance_d,
        .token = if (token) |t| (gpa.dupe(u8, t) catch null) else null,
        .project_id = project_d,
        .title = title_d,
        .description = desc_d,
        .image = if (image) |b| (gpa.dupe(u8, b) catch null) else null,
        .image_format = fmt_d,
        .toast_parent = toast_parent,
    };

    const th = std.Thread.spawn(.{}, worker, .{r}) catch {
        worker(r); // inline fallback (blocks once, still correct)
        return true;
    };
    th.detach();
    return true;
}

const ResultMsg = struct {
    gpa: std.mem.Allocator,
    ok_issue: bool,
    ok_image: bool,
    identifier: ?[]u8,
    parent: gtk.Object,
};

fn worker(r: *Report) void {
    const gpa = r.gpa;
    defer freeReport(r);

    // 1) Create the (text-only) issue.
    const created = createIssue(r) orelse {
        post(gpa, .{ .gpa = gpa, .ok_issue = false, .ok_image = false, .identifier = null, .parent = r.toast_parent });
        return;
    };
    defer {
        gpa.free(created.id);
        if (created.identifier) |i| gpa.free(i);
    }

    var ok_image = false;
    // 2 + 3) If there's a screenshot, upload it and patch the description.
    if (r.image) |img| {
        if (uploadImage(r, created.id, img)) |att_url| {
            defer gpa.free(att_url);
            ok_image = updateDescription(r, created.id, att_url);
        }
    } else {
        ok_image = true; // nothing to upload → not a failure
    }

    post(gpa, .{
        .gpa = gpa,
        .ok_issue = true,
        .ok_image = ok_image,
        .identifier = if (created.identifier) |i| (gpa.dupe(u8, i) catch null) else null,
        .parent = r.toast_parent,
    });
}

const Created = struct { id: []u8, identifier: ?[]u8 };

/// issues.create — text-only (no images; the server rejects them on create).
fn createIssue(r: *Report) ?Created {
    const gpa = r.gpa;
    const input = std.json.Stringify.valueAlloc(gpa, .{
        .projectId = r.project_id,
        .title = r.title,
        // `issueDescriptionSchema` is `z.string()` (plain GFM markdown) — the
        // legacy `{ text }` envelope was unwrapped, so send the bare string the
        // way app.zig's canonical create/update path does.
        .description = r.description,
        .status = "backlog",
    }, .{}) catch return null;
    defer gpa.free(input);

    var resp = trpc.call(gpa, r.instance, "issues.create", input, r.token, 30) catch return null;
    defer resp.deinit();
    if (!resp.ok()) return null;

    const data = resp.data() orelse return null;
    const obj = trpc.asObject(data) orelse return null;
    // The router returns either { issue: {...} } or the issue object directly;
    // accept both shapes.
    const issue_obj = if (obj.get("issue")) |iv| (trpc.asObject(iv) orelse obj) else obj;
    const id = trpc.objString(issue_obj, "id") orelse return null;
    const identifier = trpc.objString(issue_obj, "identifier");
    return .{
        .id = gpa.dupe(u8, id) catch return null,
        .identifier = if (identifier) |s| (gpa.dupe(u8, s) catch null) else null,
    };
}

/// POST /api/issues/{id}/images — multipart upload. Returns the relative
/// attachment URL ("/api/attachments/{id}") on success. Same multipart idiom as
/// markdown_editor.zig's uploadWorker.
fn uploadImage(r: *Report, issue_id: []const u8, image: []const u8) ?[]u8 {
    const gpa = r.gpa;
    const ct = contentTypeFor(r.image_format);
    const fname = std.fmt.allocPrint(gpa, "screenshot.{s}", .{r.image_format}) catch return null;
    defer gpa.free(fname);

    const header = std.fmt.allocPrint(
        gpa,
        "--{s}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{s}\"\r\nContent-Type: {s}\r\n\r\n",
        .{ boundary, fname, ct },
    ) catch return null;
    defer gpa.free(header);
    const footer = "\r\n--" ++ boundary ++ "--\r\n";

    const body = gpa.alloc(u8, header.len + image.len + footer.len) catch return null;
    defer gpa.free(body);
    @memcpy(body[0..header.len], header);
    @memcpy(body[header.len..][0..image.len], image);
    @memcpy(body[header.len + image.len ..], footer);

    const url = std.fmt.allocPrintSentinel(
        gpa,
        "{s}/api/issues/{s}/images",
        .{ std.mem.trimEnd(u8, r.instance, "/"), issue_id },
        0,
    ) catch return null;
    defer gpa.free(url);

    var resp = http.postMultipart(gpa, url, r.token, body, "multipart/form-data; boundary=" ++ boundary, 60) catch return null;
    defer resp.deinit();
    if (resp.status < 200 or resp.status >= 300) return null;

    const parsed = std.json.parseFromSlice(std.json.Value, gpa, resp.body, .{}) catch return null;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const u = trpc.objString(obj, "url") orelse return null;
    return gpa.dupe(u8, u) catch null;
}

/// issues.update — append the embedded image markdown to the description. The
/// server canonicalizes the image URL to the relative form on save.
fn updateDescription(r: *Report, issue_id: []const u8, att_url: []const u8) bool {
    const gpa = r.gpa;
    const md = std.fmt.allocPrint(gpa, "{s}\n\n![screenshot]({s})", .{ r.description, att_url }) catch return false;
    defer gpa.free(md);

    const input = std.json.Stringify.valueAlloc(gpa, .{
        .id = issue_id,
        // Plain markdown string (see createIssue) — not the legacy `{ text }`.
        .description = md,
    }, .{}) catch return false;
    defer gpa.free(input);

    var resp = trpc.call(gpa, r.instance, "issues.update", input, r.token, 30) catch return false;
    defer resp.deinit();
    return resp.ok();
}

fn contentTypeFor(fmt: []const u8) []const u8 {
    if (std.mem.eql(u8, fmt, "jpeg") or std.mem.eql(u8, fmt, "jpg")) return "image/jpeg";
    if (std.mem.eql(u8, fmt, "webp")) return "image/webp";
    return "image/png";
}

// --- result toast (marshaled back to the GTK main loop) ---

fn post(gpa: std.mem.Allocator, msg: ResultMsg) void {
    const m = gpa.create(ResultMsg) catch return;
    m.* = msg;
    _ = gtk.g_idle_add(@ptrCast(&onResult), m);
}

fn onResult(data: gtk.gpointer) callconv(.c) c_int {
    const m: *ResultMsg = @ptrCast(@alignCast(data orelse return 0));
    defer {
        if (m.identifier) |i| m.gpa.free(i);
        m.gpa.destroy(m);
    }
    // Toast UX on Linux is minimal (the run-lifecycle path also just logs); a
    // full AdwToastOverlay wiring is deferred. Log a clear outcome so the dev
    // sees what happened, matching agent_manager's stderr convention.
    if (!m.ok_issue) {
        std.debug.print("[feedback] failed to create the issue\n", .{});
    } else if (!m.ok_image) {
        std.debug.print("[feedback] issue {s} created — screenshot upload failed\n", .{m.identifier orelse "?"});
    } else {
        std.debug.print("[feedback] filed {s}\n", .{m.identifier orelse "(issue)"});
    }
    return 0; // G_SOURCE_REMOVE
}

fn freeReport(r: *Report) void {
    const gpa = r.gpa;
    gpa.free(r.instance);
    if (r.token) |t| gpa.free(t);
    gpa.free(r.project_id);
    gpa.free(r.title);
    gpa.free(r.description);
    if (r.image) |b| gpa.free(b);
    gpa.free(r.image_format);
    gpa.destroy(r);
}
