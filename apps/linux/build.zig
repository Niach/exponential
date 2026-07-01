const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Absolute path to the shared protocol fixtures so the sync-engine tests can
    // consume the exact same JSON the web/iOS/Android clients test against.
    const fixtures_dir = b.pathFromRoot("../../packages/electric-protocol/fixtures");
    const build_options = b.addOptions();
    build_options.addOption([]const u8, "fixtures_dir", fixtures_dir);

    // Absolute paths to the embeddable libghostty bundle (built once by
    // scripts/build-libghostty.sh). Passed to the app so the embedded terminal
    // can point ghostty at its resources + terminfo at runtime.
    const ghostty_install = b.pathFromRoot("vendor/ghostty-install");
    build_options.addOption([]const u8, "ghostty_resources_dir", b.fmt("{s}/share/ghostty", .{ghostty_install}));
    build_options.addOption([]const u8, "ghostty_terminfo_dir", b.fmt("{s}/share/terminfo", .{ghostty_install}));

    // Preview embedding deps are optional so the app still builds where they're
    // absent (e.g. dev boxes without WebKitGTK 6). `-Dwebkit=false` drops the
    // webview backend (web preview falls back to "open in browser"); the
    // X11 native-window reparent (Android embed) is gated by `-Dx11` (default on
    // — the gdk-x11 symbols live in gtk4 itself; we additionally link libX11 for
    // XReparentWindow). Both are read at comptime by src/ui/preview/*.zig.
    const enable_webkit = b.option(bool, "webkit", "Embed WebKitGTK 6 for the web preview backend (default: on)") orelse true;
    const enable_x11 = b.option(bool, "x11", "Enable X11 native-window reparent for the Android preview embed (default: on)") orelse true;
    build_options.addOption(bool, "enable_webkit", enable_webkit);
    build_options.addOption(bool, "enable_x11", enable_x11);

    // --- app executable (CLI + GTK GUI) ---
    const exe = b.addExecutable(.{
        .name = "exponential",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .link_libc = true,
        }),
    });
    linkCore(exe.root_module, build_options);
    linkGui(exe.root_module, enable_webkit, enable_x11);
    linkAgentCore(b, exe, optimize);
    linkLibghostty(b, exe);
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    const run_step = b.step("run", "Run the Exponential desktop app");
    run_step.dependOn(&run_cmd.step);

    // --- unit tests (core only; no GTK) ---
    const tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/tests.zig"),
            .target = target,
            .optimize = optimize,
            .link_libc = true,
        }),
    });
    linkCore(tests.root_module, build_options);
    const run_tests = b.addRunArtifact(tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_tests.step);

    // --- compile check (no linking) ---
    // Type-checks the FULL app, UI included, without linking GTK/libghostty/
    // agent-core — the GTK + ghostty bindings are hand-declared externs, so
    // compilation needs no Linux headers. This is the gate a macOS dev runs to
    // verify Linux changes "to the link boundary": `zig build check`.
    const check_obj = b.addObject(.{
        .name = "exponential-check",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .link_libc = true,
        }),
    });
    linkCore(check_obj.root_module, build_options);
    const check_step = b.step("check", "Type-check the full app without linking (runs on macOS)");
    check_step.dependOn(&check_obj.step);
}

/// Core C deps: SQLite store + libcurl networking.
fn linkCore(mod: *std.Build.Module, build_options: *std.Build.Step.Options) void {
    mod.addOptions("build_options", build_options);
    mod.linkSystemLibrary("sqlite3", .{});
    mod.linkSystemLibrary("curl", .{});
}

/// GUI C deps: GTK4 + libadwaita, plus gio for opening the OAuth URL. Cairo +
/// gdk-pixbuf back the annotation overlay draw + screenshot flatten; the optional
/// WebKitGTK 6 (web preview) and libX11 (Android native-window reparent) are
/// linked only when their build flag is on so the app still builds where they're
/// absent.
fn linkGui(mod: *std.Build.Module, enable_webkit: bool, enable_x11: bool) void {
    mod.linkSystemLibrary("gtk4", .{});
    mod.linkSystemLibrary("libadwaita-1", .{});
    mod.linkSystemLibrary("gio-2.0", .{});
    // Annotation overlay rendering + screenshot flatten.
    mod.linkSystemLibrary("cairo", .{});
    mod.linkSystemLibrary("gdk-pixbuf-2.0", .{});
    // Web preview backend. When off, src/ui/preview falls back to opening the
    // dev URL in the default browser via the existing gio path.
    if (enable_webkit) mod.linkSystemLibrary("webkitgtk-6.0", .{});
    // Android preview embed: gdk_x11_surface_get_xid is exported by gtk4 itself;
    // XReparentWindow + the property reads need libX11 directly.
    if (enable_x11) mod.linkSystemLibrary("X11", .{});
}

/// Link the embeddable libghostty (built by scripts/build-libghostty.sh into
/// vendor/ghostty-install) for the M7 embedded terminal. libghostty.so pulls in
/// libglad.so via its own DT_NEEDED ($ORIGIN rpath); we also need libGL for the
/// raw glViewport/glClear calls the render path makes.
fn linkLibghostty(b: *std.Build, exe: *std.Build.Step.Compile) void {
    const lib_dir = b.pathFromRoot("vendor/ghostty-install/lib");
    exe.root_module.addLibraryPath(.{ .cwd_relative = lib_dir });
    exe.root_module.linkSystemLibrary("ghostty", .{});
    exe.root_module.linkSystemLibrary("GL", .{});
    // Find libghostty.so (and its sibling libglad.so) at runtime.
    exe.root_module.addRPath(.{ .cwd_relative = lib_dir });
}

/// Build the Rust `agent-core` (cargo) and link its cdylib into the exe so the
/// app can drive the shared agent loop over the C ABI (ffi `agent_core_*`).
fn linkAgentCore(b: *std.Build, exe: *std.Build.Step.Compile, optimize: std.builtin.OptimizeMode) void {
    const release = optimize != .Debug;
    const manifest = b.pathFromRoot("../../Cargo.toml");
    const cargo = if (release)
        b.addSystemCommand(&.{ "cargo", "build", "--release", "-p", "agent-core", "--manifest-path", manifest })
    else
        b.addSystemCommand(&.{ "cargo", "build", "-p", "agent-core", "--manifest-path", manifest });
    // The Rust build is the source of truth; rerun it every time (cargo is
    // incremental, so this is cheap when nothing changed).
    cargo.has_side_effects = true;

    const lib_dir = b.pathFromRoot(b.fmt("../../target/{s}", .{if (release) "release" else "debug"}));
    exe.step.dependOn(&cargo.step);
    exe.root_module.addLibraryPath(.{ .cwd_relative = lib_dir });
    exe.root_module.linkSystemLibrary("agent_core", .{});
    // Find libagent_core.so at runtime (dev convenience).
    exe.root_module.addRPath(.{ .cwd_relative = lib_dir });
}
