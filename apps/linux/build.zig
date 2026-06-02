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
    linkGui(exe.root_module);
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
}

/// Core C deps: SQLite store + libcurl networking.
fn linkCore(mod: *std.Build.Module, build_options: *std.Build.Step.Options) void {
    mod.addOptions("build_options", build_options);
    mod.linkSystemLibrary("sqlite3", .{});
    mod.linkSystemLibrary("curl", .{});
}

/// GUI C deps: GTK4 + libadwaita, plus gio for opening the OAuth URL.
fn linkGui(mod: *std.Build.Module) void {
    mod.linkSystemLibrary("gtk4", .{});
    mod.linkSystemLibrary("libadwaita-1", .{});
    mod.linkSystemLibrary("gio-2.0", .{});
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
