//! Preview run-target config reader + trust gate + environment doctor.
//!
//! The CANONICAL build/run commands live ONLY in the committed
//! `.exponential/config.json` in the cloned working tree — never in the synced
//! DB mirror (`projects.preview_config`), which is display-only and never
//! executed. This module reads the repo file from the coding launcher's shared
//! clone under `<reposRoot>/<owner>/<name>` (see core/git_worktree.zig; the
//! root defaults to `~/Exponential/repos`, overridable in desktop settings)
//! and parses it into typed `RunTarget`s (mirrors
//! `packages/db-schema/src/domain.ts`).
//!
//! Trust gate: commands are agent-editable and travel with the repo, so the Run
//! button is gated behind a one-time "Trust preview commands for owner/name?"
//! prompt. We persist an approved hash of the resolved command set per repo in
//! `~/.config/exponential-desktop/preview-trust.json`; the hash changes →
//! re-prompt. This blocks a malicious synced command set while not nagging on
//! every normal agent edit (only command-set changes re-prompt).
//!
//! GTK-free (pure parse + std.fs + hashing) so the core can unit-test it; the
//! prompt UI lives in preview.zig.

const std = @import("std");
const storage = @import("../../core/storage.zig");
const credentials = @import("../../core/credentials.zig");
const contract = @import("../../core/domain/contract.generated.zig");

pub const Platform = enum {
    web,
    android,
    ios,
    command,

    pub fn fromString(s: []const u8) ?Platform {
        if (std.mem.eql(u8, s, contract.platform_web)) return .web;
        if (std.mem.eql(u8, s, contract.platform_android)) return .android;
        if (std.mem.eql(u8, s, contract.platform_ios)) return .ios;
        if (std.mem.eql(u8, s, contract.platform_command)) return .command;
        return null;
    }

    pub fn label(self: Platform) []const u8 {
        return switch (self) {
            .web => "Web",
            .android => "Android",
            .ios => "iOS",
            .command => "Command",
        };
    }
};

/// One parsed run target. All optional command fields are arena-owned slices
/// into the parse arena (so a Config's whole lifetime is the arena). Platform
/// selects which command fields are meaningful (mirrors the discriminated union
/// in domain.ts); we keep a flat struct since the consumer dispatches on
/// `platform` anyway.
pub const RunTarget = struct {
    id: []const u8,
    name: []const u8,
    platform: Platform,

    // PlatformCommon
    enabled: bool = true,
    root_dir: ?[]const u8 = null,
    setup: ?[]const u8 = null,
    /// Extra child env (key, value). PATH / LD_PRELOAD / LD_LIBRARY_PATH /
    /// DYLD_* are stripped at parse time (never spawned, never hashed).
    env: []const [2][]const u8 = &.{},

    // web
    run: ?[]const u8 = null,
    url: ?[]const u8 = null,
    port: ?i64 = null,
    ready_path: ?[]const u8 = null,
    inject_widget: bool = false,

    // android
    build: ?[]const u8 = null,
    apk: ?[]const u8 = null,
    install_command: ?[]const u8 = null,
    avd: ?[]const u8 = null,
    application_id: ?[]const u8 = null,
    activity: ?[]const u8 = null,

    // ios
    scheme: ?[]const u8 = null,
    workspace: ?[]const u8 = null,
    simulator: ?[]const u8 = null,
    bundle_id: ?[]const u8 = null,

    // command (masterplan §4c: generic host-side process, spawned as-is — no
    // shell interpretation of the elements themselves)
    argv: ?[]const []const u8 = null,
    /// Working directory relative to the repo root. `..` and absolute paths
    /// are rejected at parse time (the target is dropped).
    cwd: ?[]const u8 = null,
};

pub const Config = struct {
    version: i64 = 1,
    targets: []RunTarget,
};

pub const Error = error{ NotFound, ParseFailed, OutOfMemory };

/// `<reposRoot>/<repo_slug>` — the coding launcher's shared clone
/// (git_worktree.clonePath). `repo_slug` is the repositories-registry
/// `fullName` ("owner/name"). Caller owns the returned path.
pub fn repoCloneDir(gpa: std.mem.Allocator, repo_slug: []const u8) ![]u8 {
    var cred = credentials.Store.open(gpa) catch return credentialLessCloneDir(gpa, repo_slug);
    defer cred.deinit();
    const root = try cred.reposRoot(gpa);
    defer gpa.free(root);
    return std.fs.path.join(gpa, &.{ root, repo_slug });
}

/// Fallback when the desktop-settings store can't be opened: the default root.
fn credentialLessCloneDir(gpa: std.mem.Allocator, repo_slug: []const u8) ![]u8 {
    const root = try credentials.defaultReposRoot(gpa);
    defer gpa.free(root);
    return std.fs.path.join(gpa, &.{ root, repo_slug });
}

/// Absolute path to a clone's `.exponential/config.json`. Caller owns it.
pub fn configPath(gpa: std.mem.Allocator, repo_slug: []const u8) ![]u8 {
    const clone = try repoCloneDir(gpa, repo_slug);
    defer gpa.free(clone);
    return std.fs.path.join(gpa, &.{ clone, ".exponential", "config.json" });
}

/// Read + parse the repo file for `repo_slug`. Everything is allocated into
/// `arena` (caller owns the arena; free it to free the whole Config). Returns
/// NotFound when the clone has no config file (the project simply has no preview
/// targets yet).
pub fn load(arena: std.mem.Allocator, repo_slug: []const u8) Error!Config {
    const path = configPath(arena, repo_slug) catch return Error.OutOfMemory;
    const raw = storage.readFileAlloc(arena, path) orelse return Error.NotFound;
    return parse(arena, raw);
}

/// Parse `.exponential/config.json` content. Unknown/extra fields are ignored
/// (forward-compatible). Targets missing their discriminator or a required
/// command field for the platform are dropped rather than failing the whole
/// parse (a partial config still previews the valid targets).
pub fn parse(arena: std.mem.Allocator, raw: []const u8) Error!Config {
    const parsed = std.json.parseFromSliceLeaky(std.json.Value, arena, raw, .{}) catch
        return Error.ParseFailed;
    const obj = switch (parsed) {
        .object => |o| o,
        else => return Error.ParseFailed,
    };
    const targets_v = obj.get("targets") orelse return Error.ParseFailed;
    const arr = switch (targets_v) {
        .array => |a| a,
        else => return Error.ParseFailed,
    };

    var out = std.ArrayList(RunTarget).empty;
    for (arr.items) |item| {
        const t = parseTarget(arena, item) orelse continue;
        out.append(arena, t) catch return Error.OutOfMemory;
    }

    var version: i64 = 1;
    if (obj.get("version")) |v| if (v == .integer) {
        version = v.integer;
    };
    return .{ .version = version, .targets = out.toOwnedSlice(arena) catch return Error.OutOfMemory };
}

fn parseTarget(arena: std.mem.Allocator, item: std.json.Value) ?RunTarget {
    const o = switch (item) {
        .object => |x| x,
        else => return null,
    };
    const platform = Platform.fromString(str(o, "platform") orelse return null) orelse return null;
    const id = str(o, "id") orelse return null;
    const name = str(o, "name") orelse id;

    var t = RunTarget{ .id = id, .name = name, .platform = platform };
    t.enabled = if (o.get("enabled")) |v| (v != .bool or v.bool) else true;
    t.root_dir = str(o, "rootDir");
    t.setup = str(o, "setup");
    t.env = parseEnv(arena, o);
    switch (platform) {
        .web => {
            t.run = str(o, "run");
            t.url = str(o, "url");
            t.port = int(o, "port");
            t.ready_path = str(o, "readyPath");
            t.inject_widget = boolField(o, "injectWidget");
        },
        .android => {
            t.build = str(o, "build");
            t.apk = str(o, "apk");
            t.install_command = str(o, "installCommand");
            t.avd = str(o, "avd");
            t.application_id = str(o, "applicationId");
            t.activity = str(o, "activity");
        },
        .ios => {
            t.scheme = str(o, "scheme");
            t.workspace = str(o, "workspace");
            t.simulator = str(o, "simulator");
            t.bundle_id = str(o, "bundleId");
        },
        .command => {
            // argv is required (min 1, strings only) — an invalid one drops
            // the target rather than failing the whole parse.
            t.argv = parseArgv(arena, o) orelse return null;
            if (str(o, "cwd")) |c| {
                if (!isSafeRelDir(c)) return null;
                t.cwd = c;
            }
        },
    }
    return t;
}

/// `argv` must be a non-empty array of strings (mirrors the zod
/// `z.array(z.string()).min(1)`); anything else invalidates the target.
fn parseArgv(arena: std.mem.Allocator, o: std.json.ObjectMap) ?[]const []const u8 {
    const v = o.get("argv") orelse return null;
    const arr = switch (v) {
        .array => |a| a,
        else => return null,
    };
    if (arr.items.len == 0) return null;
    var out = std.ArrayList([]const u8).empty;
    for (arr.items) |it| switch (it) {
        .string => |s| out.append(arena, s) catch return null,
        else => return null,
    };
    return out.toOwnedSlice(arena) catch null;
}

/// Common `env` map → (key, value) pairs, dropping loader-hijack keys
/// (PATH / LD_PRELOAD / LD_LIBRARY_PATH / DYLD_*) so a repo-carried config
/// can't swap the binaries the host resolves. Order follows the document.
fn parseEnv(arena: std.mem.Allocator, o: std.json.ObjectMap) []const [2][]const u8 {
    const v = o.get("env") orelse return &.{};
    const m = switch (v) {
        .object => |x| x,
        else => return &.{},
    };
    var out = std.ArrayList([2][]const u8).empty;
    var it = m.iterator();
    while (it.next()) |e| {
        if (e.value_ptr.* != .string) continue;
        if (isBlockedEnvKey(e.key_ptr.*)) continue;
        out.append(arena, .{ e.key_ptr.*, e.value_ptr.string }) catch continue;
    }
    return out.toOwnedSlice(arena) catch &.{};
}

fn isBlockedEnvKey(key: []const u8) bool {
    return std.mem.eql(u8, key, "PATH") or
        std.mem.eql(u8, key, "LD_PRELOAD") or
        std.mem.eql(u8, key, "LD_LIBRARY_PATH") or
        std.mem.startsWith(u8, key, "DYLD_");
}

/// Repo-relative directory: non-empty, not absolute, no `..` segment.
pub fn isSafeRelDir(p: []const u8) bool {
    if (p.len == 0) return false;
    if (std.fs.path.isAbsolute(p)) return false;
    var it = std.mem.tokenizeScalar(u8, p, '/');
    while (it.next()) |seg| {
        if (std.mem.eql(u8, seg, "..")) return false;
    }
    return true;
}

fn str(o: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = o.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}
fn int(o: std.json.ObjectMap, key: []const u8) ?i64 {
    const v = o.get(key) orelse return null;
    return switch (v) {
        .integer => |i| i,
        .float => |f| @intFromFloat(f),
        else => null,
    };
}
fn boolField(o: std.json.ObjectMap, key: []const u8) bool {
    const v = o.get(key) orelse return false;
    return v == .bool and v.bool;
}

// =========================================================================
// Trust gate
// =========================================================================
//
// The approved command set is hashed per repo and stored in
// `preview-trust.json` as a flat map { "<repo_slug>": "<hex sha256>" }. A repo
// is trusted iff its current command-set hash matches the stored one. The hash
// covers every command string that would actually be executed (setup/run/build/
// install) across all targets, plus rootDir/avd/applicationId — i.e. everything
// that influences what a Run does — so a malicious edit to any of them
// re-prompts, while pure metadata churn (display name) does not.

/// Stable SHA-256 (hex) of the executable command surface of a parsed config.
pub fn commandSetHash(gpa: std.mem.Allocator, cfg: Config) ![]u8 {
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    for (cfg.targets) |t| {
        // Field-tagged so reordering/empty fields can't collide.
        hashField(&hasher, "id", t.id);
        hashField(&hasher, "platform", @tagName(t.platform));
        hashOpt(&hasher, "rootDir", t.root_dir);
        hashOpt(&hasher, "setup", t.setup);
        hashOpt(&hasher, "run", t.run);
        hashOpt(&hasher, "build", t.build);
        hashOpt(&hasher, "install", t.install_command);
        hashOpt(&hasher, "apk", t.apk);
        hashOpt(&hasher, "avd", t.avd);
        hashOpt(&hasher, "applicationId", t.application_id);
        hashOpt(&hasher, "activity", t.activity);
        hashOpt(&hasher, "scheme", t.scheme);
        hashOpt(&hasher, "workspace", t.workspace);
        // command surface (§4c): argv + cwd + env all influence what a Run
        // executes, so any edit re-prompts the trust gate.
        if (t.argv) |argv| {
            var ibuf: [24]u8 = undefined;
            for (argv, 0..) |arg, i| {
                const key = std.fmt.bufPrint(&ibuf, "argv{d}", .{i}) catch "argv";
                hashField(&hasher, key, arg);
            }
        }
        hashOpt(&hasher, "cwd", t.cwd);
        for (t.env) |kv| {
            hashField(&hasher, "envKey", kv[0]);
            hashField(&hasher, "envVal", kv[1]);
        }
    }
    var digest: [32]u8 = undefined;
    hasher.final(&digest);
    return std.fmt.allocPrint(gpa, "{x}", .{&digest});
}

fn hashField(h: *std.crypto.hash.sha2.Sha256, key: []const u8, val: []const u8) void {
    h.update(key);
    h.update("=");
    h.update(val);
    h.update("\n");
}
fn hashOpt(h: *std.crypto.hash.sha2.Sha256, key: []const u8, val: ?[]const u8) void {
    hashField(h, key, val orelse "");
}

fn trustStorePath(gpa: std.mem.Allocator) ![]u8 {
    return storeFilePath(gpa, "preview-trust.json");
}

fn storeFilePath(gpa: std.mem.Allocator, name: []const u8) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    return std.fs.path.join(gpa, &.{ dir, name });
}

/// Read one string value out of a flat `{ "<key>": "<string>" }` store file.
/// The returned slice borrows `a` (an arena).
fn readStoreValue(a: std.mem.Allocator, path: []const u8, key: []const u8) ?[]const u8 {
    const raw = storage.readFileAlloc(a, path) orelse return null;
    const parsed = std.json.parseFromSliceLeaky(std.json.Value, a, raw, .{}) catch return null;
    const obj = switch (parsed) {
        .object => |o| o,
        else => return null,
    };
    return str(obj, key);
}

/// Merge `key`→`value` into a flat string-map store file, preserving the other
/// entries. Best-effort (a write failure just loses the memo).
fn writeStoreValue(gpa: std.mem.Allocator, name: []const u8, key: []const u8, value: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const path = storeFilePath(a, name) catch return;

    // Load the existing map as a JSON object, then overwrite this key and
    // re-serialize. Building on the parsed ObjectMap keeps all string values
    // arena-borrowed (valid until we write).
    var obj: std.json.ObjectMap = .empty;
    if (storage.readFileAlloc(a, path)) |raw| {
        if (std.json.parseFromSliceLeaky(std.json.Value, a, raw, .{})) |parsed| {
            if (parsed == .object) {
                var it = parsed.object.iterator();
                while (it.next()) |e| {
                    if (e.value_ptr.* == .string) obj.put(a, e.key_ptr.*, e.value_ptr.*) catch {};
                }
            }
        } else |_| {}
    }
    obj.put(a, key, std.json.Value{ .string = value }) catch return;

    const out = std.json.Stringify.valueAlloc(a, std.json.Value{ .object = obj }, .{}) catch return;
    storage.writeSecret(path, out) catch {};
}

/// Whether `repo_slug`'s current command-set `hash` is already approved.
pub fn isTrusted(gpa: std.mem.Allocator, repo_slug: []const u8, hash: []const u8) bool {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const path = trustStorePath(a) catch return false;
    const stored = readStoreValue(a, path, repo_slug) orelse return false;
    return std.mem.eql(u8, stored, hash);
}

/// Record `repo_slug`→`hash` as approved (called after the user accepts the
/// trust prompt). Merges into the existing store; best-effort (a write failure
/// just means we re-prompt next time).
pub fn trust(gpa: std.mem.Allocator, repo_slug: []const u8, hash: []const u8) void {
    writeStoreValue(gpa, "preview-trust.json", repo_slug, hash);
}

// --- last-selected run target (play menu memory, §4c) ---
//
// A sibling `last-run.json` beside the trust store: flat map
// { "<repo_slug>": "<target id>" } — which run config the play menu used last
// for each repo. Pure UI memory, no security relevance.

/// The last-selected run-target id for `repo_slug` (caller owns the copy).
pub fn lastSelectedTarget(gpa: std.mem.Allocator, repo_slug: []const u8) ?[]u8 {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const path = storeFilePath(a, "last-run.json") catch return null;
    const stored = readStoreValue(a, path, repo_slug) orelse return null;
    return gpa.dupe(u8, stored) catch null;
}

/// Persist `repo_slug`→`target_id` as the play menu's last selection.
pub fn setLastSelectedTarget(gpa: std.mem.Allocator, repo_slug: []const u8, target_id: []const u8) void {
    writeStoreValue(gpa, "last-run.json", repo_slug, target_id);
}

// =========================================================================
// Doctor — probe the local toolchain a target needs. Pure std.process probes
// (no GTK); the UI renders the resulting checklist.
// =========================================================================

pub const DoctorCheck = struct {
    label: []const u8,
    ok: bool,
    detail: ?[]const u8 = null,
};

/// Run the doctor checks for `platform`. Results are arena-owned. Probes are
/// silent (`std.process.Child` with piped output), never the visible terminal.
pub fn doctor(arena: std.mem.Allocator, platform: Platform) []DoctorCheck {
    var out = std.ArrayList(DoctorCheck).empty;
    switch (platform) {
        .android => {
            const sdk = envVar(arena, "ANDROID_HOME") orelse envVar(arena, "ANDROID_SDK_ROOT");
            out.append(arena, .{
                .label = "Android SDK (ANDROID_HOME)",
                .ok = sdk != null,
                .detail = sdk,
            }) catch {};
            out.append(arena, .{ .label = "emulator on PATH", .ok = which(arena, "emulator") }) catch {};
            out.append(arena, .{ .label = "adb on PATH", .ok = which(arena, "adb") }) catch {};
            // AVDs: `emulator -list-avds` (one per line).
            const avds = listAvds(arena);
            out.append(arena, .{
                .label = "At least one AVD",
                .ok = avds.len > 0,
                .detail = if (avds.len > 0) avds else "run `avdmanager create avd ...`",
            }) catch {};
            out.append(arena, .{
                .label = "WebKitGTK 6 (not needed for Android)",
                .ok = true,
            }) catch {};
        },
        .web => {
            // The web backend prefers WebKitGTK 6; if absent we fall back to
            // opening the URL in the browser, so this is informational.
            out.append(arena, .{
                .label = "webkitgtk-6.0 (embedded webview)",
                .ok = pkgExists(arena, "webkitgtk-6.0"),
                .detail = "absent → web preview opens in your browser instead",
            }) catch {};
        },
        .ios => {
            out.append(arena, .{
                .label = "iOS preview",
                .ok = false,
                .detail = "needs a Mac — local iOS emulation isn't possible on Linux",
            }) catch {};
        },
        .command => {
            // Command targets run `sh <launcher script>` in a terminal tab.
            out.append(arena, .{
                .label = "POSIX sh on PATH",
                .ok = which(arena, "sh"),
                .detail = "command targets run in a terminal-dock tab",
            }) catch {};
        },
    }
    return out.toOwnedSlice(arena) catch &.{};
}

fn envVar(arena: std.mem.Allocator, name: []const u8) ?[]const u8 {
    const z = std.fmt.allocPrintSentinel(arena, "{s}", .{name}, 0) catch return null;
    const v = std.c.getenv(z.ptr) orelse return null;
    const s = std.mem.span(v);
    return if (s.len == 0) null else s;
}

/// Whether `prog` resolves on PATH (silent `command -v`).
fn which(arena: std.mem.Allocator, prog: []const u8) bool {
    const out = runSilent(arena, &.{ "/usr/bin/env", "sh", "-c", "command -v \"$0\" >/dev/null 2>&1", prog }) orelse return false;
    return out.term == .exited and out.term.exited == 0;
}

fn pkgExists(arena: std.mem.Allocator, pkg: []const u8) bool {
    const out = runSilent(arena, &.{ "pkg-config", "--exists", pkg }) orelse return false;
    return out.term == .exited and out.term.exited == 0;
}

fn listAvds(arena: std.mem.Allocator) []const u8 {
    const out = runSilent(arena, &.{ "emulator", "-list-avds" }) orelse return "";
    if (!(out.term == .exited and out.term.exited == 0)) return "";
    return std.mem.trim(u8, out.stdout, " \t\r\n");
}

const RunOut = struct { term: std.process.Child.Term, stdout: []const u8 };

/// Capture a short command's output, swallowing spawn failures. Used only for
/// quick silent probes (never long-running). arena-owned stdout.
fn runSilent(arena: std.mem.Allocator, argv: []const []const u8) ?RunOut {
    const res = std.process.run(arena, std.Io.Threaded.global_single_threaded.io(), .{
        .argv = argv,
        .stdout_limit = .limited(64 * 1024),
        .stderr_limit = .limited(64 * 1024),
    }) catch return null;
    return .{ .term = res.term, .stdout = res.stdout };
}

// =========================================================================
// Tests (GTK-free; exercised by the core test root).
// =========================================================================

test "parse picks up multiple targets per platform and ignores unknowns" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const raw =
        \\{ "version": 1, "futureField": true, "targets": [
        \\  { "id": "web", "name": "Web", "platform": "web", "run": "bun dev", "url": "http://localhost:5173", "injectWidget": true },
        \\  { "id": "android", "name": "Android", "platform": "android", "build": "./gradlew assembleDebug", "avd": "Pixel_7", "applicationId": "at.yourev.exponential" },
        \\  { "id": "ios-staging", "name": "iOS Staging", "platform": "ios", "scheme": "Exp-Staging", "simulator": "iPhone 15", "bundleId": "at.yourev.s" },
        \\  { "id": "ios-prod", "name": "iOS Prod", "platform": "ios", "scheme": "Exp", "simulator": "iPhone 15", "bundleId": "at.yourev" },
        \\  { "id": "bad", "platform": "nope" }
        \\]}
    ;
    const cfg = try parse(a, raw);
    try std.testing.expectEqual(@as(usize, 4), cfg.targets.len);
    try std.testing.expectEqual(Platform.web, cfg.targets[0].platform);
    try std.testing.expect(cfg.targets[0].inject_widget);
    try std.testing.expectEqualStrings("Pixel_7", cfg.targets[1].avd.?);
    // Two ios targets (staging + prod) coexist.
    try std.testing.expectEqual(Platform.ios, cfg.targets[2].platform);
    try std.testing.expectEqual(Platform.ios, cfg.targets[3].platform);
}

test "commandSetHash changes when a command changes but not on name churn" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const base = try parse(a,
        \\{ "targets": [ { "id": "web", "name": "Web", "platform": "web", "run": "bun dev" } ] }
    );
    const renamed = try parse(a,
        \\{ "targets": [ { "id": "web", "name": "Web Renamed", "platform": "web", "run": "bun dev" } ] }
    );
    const changed = try parse(a,
        \\{ "targets": [ { "id": "web", "name": "Web", "platform": "web", "run": "rm -rf /" } ] }
    );
    const h_base = try commandSetHash(a, base);
    const h_renamed = try commandSetHash(a, renamed);
    const h_changed = try commandSetHash(a, changed);
    try std.testing.expectEqualStrings(h_base, h_renamed); // name doesn't affect trust
    try std.testing.expect(!std.mem.eql(u8, h_base, h_changed)); // command does
}

test "parse command target: argv/cwd/env land, invalid ones are dropped" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const raw =
        \\{ "targets": [
        \\  { "id": "test", "name": "Unit tests", "platform": "command",
        \\    "argv": ["zig", "build", "test"], "cwd": "apps/linux",
        \\    "env": { "FOO": "bar", "PATH": "/evil", "LD_PRELOAD": "/evil.so", "DYLD_INSERT_LIBRARIES": "x", "OK2": "v" } },
        \\  { "id": "no-argv", "platform": "command" },
        \\  { "id": "empty-argv", "platform": "command", "argv": [] },
        \\  { "id": "bad-argv", "platform": "command", "argv": ["ok", 3] },
        \\  { "id": "escape", "platform": "command", "argv": ["ls"], "cwd": "../outside" },
        \\  { "id": "abs", "platform": "command", "argv": ["ls"], "cwd": "/etc" },
        \\  { "id": "plain", "platform": "command", "argv": ["make"] }
        \\]}
    ;
    const cfg = try parse(a, raw);
    try std.testing.expectEqual(@as(usize, 2), cfg.targets.len);

    const t = cfg.targets[0];
    try std.testing.expectEqual(Platform.command, t.platform);
    try std.testing.expectEqual(@as(usize, 3), t.argv.?.len);
    try std.testing.expectEqualStrings("zig", t.argv.?[0]);
    try std.testing.expectEqualStrings("test", t.argv.?[2]);
    try std.testing.expectEqualStrings("apps/linux", t.cwd.?);
    // env keeps FOO/OK2 but strips the loader-hijack keys.
    try std.testing.expectEqual(@as(usize, 2), t.env.len);
    try std.testing.expectEqualStrings("FOO", t.env[0][0]);
    try std.testing.expectEqualStrings("bar", t.env[0][1]);
    try std.testing.expectEqualStrings("OK2", t.env[1][0]);

    const plain = cfg.targets[1];
    try std.testing.expectEqualStrings("plain", plain.id);
    try std.testing.expect(plain.cwd == null);
    try std.testing.expectEqual(@as(usize, 0), plain.env.len);
}

test "commandSetHash covers argv, cwd and env of command targets" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const base = try parse(a,
        \\{ "targets": [ { "id": "t", "name": "T", "platform": "command", "argv": ["zig", "build"], "cwd": "apps/linux", "env": { "FOO": "1" } } ] }
    );
    const renamed = try parse(a,
        \\{ "targets": [ { "id": "t", "name": "T renamed", "platform": "command", "argv": ["zig", "build"], "cwd": "apps/linux", "env": { "FOO": "1" } } ] }
    );
    const argv_changed = try parse(a,
        \\{ "targets": [ { "id": "t", "name": "T", "platform": "command", "argv": ["zig", "build", "-Devil"], "cwd": "apps/linux", "env": { "FOO": "1" } } ] }
    );
    const cwd_changed = try parse(a,
        \\{ "targets": [ { "id": "t", "name": "T", "platform": "command", "argv": ["zig", "build"], "cwd": "apps", "env": { "FOO": "1" } } ] }
    );
    const env_changed = try parse(a,
        \\{ "targets": [ { "id": "t", "name": "T", "platform": "command", "argv": ["zig", "build"], "cwd": "apps/linux", "env": { "FOO": "2" } } ] }
    );
    // ["a b"] must not collide with ["a", "b"] (element boundaries are hashed).
    const argv_joined = try parse(a,
        \\{ "targets": [ { "id": "t", "name": "T", "platform": "command", "argv": ["zig build"], "cwd": "apps/linux", "env": { "FOO": "1" } } ] }
    );

    const h_base = try commandSetHash(a, base);
    try std.testing.expectEqualStrings(h_base, try commandSetHash(a, renamed));
    try std.testing.expect(!std.mem.eql(u8, h_base, try commandSetHash(a, argv_changed)));
    try std.testing.expect(!std.mem.eql(u8, h_base, try commandSetHash(a, cwd_changed)));
    try std.testing.expect(!std.mem.eql(u8, h_base, try commandSetHash(a, env_changed)));
    try std.testing.expect(!std.mem.eql(u8, h_base, try commandSetHash(a, argv_joined)));
}

test "isSafeRelDir rejects escapes and absolutes" {
    try std.testing.expect(isSafeRelDir("apps/linux"));
    try std.testing.expect(isSafeRelDir("a/b/c"));
    try std.testing.expect(!isSafeRelDir(""));
    try std.testing.expect(!isSafeRelDir("/etc"));
    try std.testing.expect(!isSafeRelDir(".."));
    try std.testing.expect(!isSafeRelDir("a/../b"));
    try std.testing.expect(!isSafeRelDir("a/.."));
}
