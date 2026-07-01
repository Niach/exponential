import AppKit
import Foundation
import GhosttyKit
import os

private let log = Logger(subsystem: "at.exponential.mac", category: "GhosttyTerminal")

/// An embedded libghostty terminal surface in an NSView (Metal, managed by the
/// ghostty macOS apprt). Runs one `command` and reports completion. Mirrors the
/// macterm GhosttyTerminalNSView, trimmed to what the agent run needs (no IME /
/// splits / search). Input is forwarded so the user can steer the agent.
final class MacGhosttyTerminalView: NSView {
    nonisolated(unsafe) private(set) var surface: ghostty_surface_t?
    var onCommandFinished: ((Int16) -> Void)?
    var onProcessExit: (() -> Void)?
    var onTitleChange: ((String) -> Void)?

    private let command: String
    private let cwd: String?
    private var isFocused = false

    init(command: String, cwd: String?) {
        self.command = command
        self.cwd = cwd
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Surface lifecycle

    func createSurface() {
        guard surface == nil, let app = MacGhosttyApp.shared.app else { return }
        let backing = convertToBacking(bounds).size
        guard backing.width > 0, backing.height > 0 else { return } // retry on resize

        var config = ghostty_surface_config_new()
        config.platform_tag = GHOSTTY_PLATFORM_MACOS
        config.platform = ghostty_platform_u(macos: ghostty_platform_macos_s(nsview: Unmanaged.passUnretained(self).toOpaque()))
        config.userdata = Unmanaged.passUnretained(self).toOpaque()
        config.scale_factor = Double(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2.0)
        config.context = GHOSTTY_SURFACE_CONTEXT_WINDOW
        config.wait_after_command = true

        command.withCString { cmd in
            config.command = cmd
            if let cwd {
                cwd.withCString { wd in
                    config.working_directory = wd
                    surface = ghostty_surface_new(app, &config)
                }
            } else {
                surface = ghostty_surface_new(app, &config)
            }
        }
        guard let surface else {
            log.error("ghostty_surface_new failed")
            return
        }
        ghostty_surface_set_focus(surface, isFocused)
        updateSize()
    }

    func destroySurface() {
        if let surface { ghostty_surface_free(surface) }
        surface = nil
    }

    deinit { if let surface { ghostty_surface_free(surface) } }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil else { return }
        if surface == nil { createSurface() } else { updateSize() }
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if surface == nil { createSurface() }
        updateSize()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        updateSize()
    }

    private func updateSize() {
        guard let surface, window != nil else { return }
        let sz = convertToBacking(bounds).size
        guard sz.width > 0, sz.height > 0 else { return }
        let scale = Double(window?.backingScaleFactor ?? 2.0)
        layer?.contentsScale = CGFloat(scale)
        ghostty_surface_set_content_scale(surface, scale, scale)
        ghostty_surface_set_size(surface, UInt32(sz.width), UInt32(sz.height))
    }

    // MARK: - Focus

    override var acceptsFirstResponder: Bool { true }

    override func becomeFirstResponder() -> Bool {
        let r = super.becomeFirstResponder()
        if r, let surface { isFocused = true; ghostty_surface_set_focus(surface, true) }
        return r
    }

    override func resignFirstResponder() -> Bool {
        let r = super.resignFirstResponder()
        if r, let surface { isFocused = false; ghostty_surface_set_focus(surface, false) }
        return r
    }

    // MARK: - Input forwarding (basic; enough to steer the agent)

    private func mods(_ e: NSEvent) -> ghostty_input_mods_e {
        var m = GHOSTTY_MODS_NONE.rawValue
        let f = e.modifierFlags
        if f.contains(.shift) { m |= GHOSTTY_MODS_SHIFT.rawValue }
        if f.contains(.control) { m |= GHOSTTY_MODS_CTRL.rawValue }
        if f.contains(.option) { m |= GHOSTTY_MODS_ALT.rawValue }
        if f.contains(.command) { m |= GHOSTTY_MODS_SUPER.rawValue }
        if f.contains(.capsLock) { m |= GHOSTTY_MODS_CAPS.rawValue }
        return ghostty_input_mods_e(rawValue: m)
    }

    private func keyEvent(_ e: NSEvent, _ action: ghostty_input_action_e) -> ghostty_input_key_s {
        var ke = ghostty_input_key_s()
        ke.action = action
        ke.keycode = UInt32(e.keyCode)
        ke.mods = mods(e)
        ke.consumed_mods = GHOSTTY_MODS_NONE
        ke.composing = false
        ke.text = nil
        if let s = e.charactersIgnoringModifiers?.unicodeScalars.first { ke.unshifted_codepoint = s.value }
        return ke
    }

    override func keyDown(with e: NSEvent) {
        guard let surface else { return }
        var ke = keyEvent(e, e.isARepeat ? GHOSTTY_ACTION_REPEAT : GHOSTTY_ACTION_PRESS)
        let text = e.characters ?? ""
        if text.isEmpty {
            ke.text = nil
            _ = ghostty_surface_key(surface, ke)
        } else {
            text.withCString { ke.text = $0; _ = ghostty_surface_key(surface, ke) }
        }
    }

    override func keyUp(with e: NSEvent) {
        guard let surface else { return }
        var ke = keyEvent(e, GHOSTTY_ACTION_RELEASE)
        ke.text = nil
        _ = ghostty_surface_key(surface, ke)
    }

    override func flagsChanged(with e: NSEvent) {
        guard let surface else { return }
        var ke = keyEvent(e, GHOSTTY_ACTION_PRESS)
        ke.text = nil
        _ = ghostty_surface_key(surface, ke)
    }

    // MARK: - Remote steer input injection (masterplan §3.3)

    /// Write UTF-8 bytes into the surface's PTY — the SAME path a paste reaches
    /// `claude` through — so a remote steerer's keystrokes merge with local input
    /// on one stream. libghostty owns the PTY; `ghostty_surface_text` is the only
    /// write seam (there is no host-side PTY master fd to write to on macOS).
    func writeToPty(_ text: String) {
        guard let surface, !text.isEmpty else { return }
        let bytes = Array(text.utf8)
        bytes.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return }
            ghostty_surface_text(surface, base.assumingMemoryBound(to: CChar.self), UInt(bytes.count))
        }
    }

    private func mousePoint(_ e: NSEvent) -> NSPoint {
        let p = convert(e.locationInWindow, from: nil)
        return NSPoint(x: p.x, y: bounds.height - p.y)
    }

    override func mouseDown(with e: NSEvent) {
        guard let surface else { return }
        window?.makeFirstResponder(self)
        let pt = mousePoint(e)
        ghostty_surface_mouse_pos(surface, pt.x, pt.y, mods(e))
        _ = ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_LEFT, mods(e))
    }

    override func mouseUp(with e: NSEvent) {
        guard let surface else { return }
        let pt = mousePoint(e)
        ghostty_surface_mouse_pos(surface, pt.x, pt.y, mods(e))
        _ = ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_LEFT, mods(e))
    }

    override func mouseDragged(with e: NSEvent) { mouseMoved(with: e) }

    override func mouseMoved(with e: NSEvent) {
        guard let surface else { return }
        let pt = mousePoint(e)
        ghostty_surface_mouse_pos(surface, pt.x, pt.y, mods(e))
    }

    override func scrollWheel(with e: NSEvent) {
        guard let surface else { return }
        var scrollMods: ghostty_input_scroll_mods_t = 0
        if e.hasPreciseScrollingDeltas { scrollMods |= 1 }
        ghostty_surface_mouse_scroll(surface, e.scrollingDeltaX, e.scrollingDeltaY, scrollMods)
    }
}

/// Runs one host-side process inside a visible ghostty terminal window,
/// capturing output + exit code via the same tee/PIPESTATUS bash wrapper the
/// Linux app uses, then reports the result to the caller (preview/run-config
/// backends today; the "Start coding" launcher later).
/// Owns one in-flight run: the capture file paths, the result callback,
/// and the terminal window (as its delegate). `@MainActor` so all access is
/// main-isolated; the window's weak `delegate` is kept alive by the runner.
@MainActor
final class TerminalRunSession: NSObject, NSWindowDelegate {
    private let promptPath, scriptPath, outPath, codePath: String
    private let onDone: @Sendable (Int32, String) -> Void
    // STRONG: the ghostty surface holds an unretained pointer to this view as its
    // userdata, so the view must outlive the surface. The session is retained by
    // the runner until windowWillClose frees the surface, then released — so a
    // callback can never deref a freed view.
    let view: MacGhosttyTerminalView
    var onClosed: (() -> Void)?
    private var finished = false

    init(view: MacGhosttyTerminalView, promptPath: String, scriptPath: String, outPath: String, codePath: String,
         onDone: @escaping @Sendable (Int32, String) -> Void) {
        self.view = view
        self.promptPath = promptPath
        self.scriptPath = scriptPath
        self.outPath = outPath
        self.codePath = codePath
        self.onDone = onDone
    }

    /// Called once (command finished, surface closed, or window closed). Prefers
    /// the wrapper-recorded PIPESTATUS exit code over ghostty's (always 0 for the
    /// wrapper itself).
    func finish(_ ghosttyCode: Int32) {
        guard !finished else { return }
        finished = true
        let code = (try? String(contentsOfFile: codePath, encoding: .utf8))
            .flatMap { Int32($0.trimmingCharacters(in: .whitespacesAndNewlines)) } ?? ghosttyCode
        let out = (try? String(contentsOfFile: outPath, encoding: .utf8)) ?? ""
        onDone(code, out)
        for p in [promptPath, scriptPath, codePath, outPath] { try? FileManager.default.removeItem(atPath: p) }
    }

    func windowWillClose(_ notification: Notification) {
        finish(-1) // safety net if the user closes before the command finished
        view.destroySurface() // frees the surface BEFORE the strong view ref drops
        onClosed?()
    }
}

@MainActor
final class MacTerminalRunner {
    static let shared = MacTerminalRunner()
    private var sessions: [String: TerminalRunSession] = [:]
    private var windows: [String: NSWindow] = [:]
    /// The shared bottom terminal dock (set once by `MacAppDependencies`).
    /// Interactive runs mount here; headless runs use a per-run window.
    weak var dock: MacTerminalDock?

    /// `onDone(exitCode, capturedOutput)` is always called exactly once. Interactive
    /// runs mount in the dock and run WITHOUT capture (empty output); headless runs
    /// open a window and capture stdout via the tee/PIPESTATUS wrapper.
    func run(
        runId: String,
        program: String,
        argv: [String],
        env: [String: String],
        cwd: String?,
        prompt: String,
        interactive: Bool = false,
        issueIdentifier: String = "",
        onDone: @escaping @Sendable (Int32, String) -> Void
    ) {
        guard MacGhosttyApp.shared.app != nil else {
            // No terminal engine (e.g. GhosttyKit not fetched) — nothing can host
            // the command. Report failure so the caller (the preview run) surfaces
            // it instead of hanging on a result that never arrives.
            onDone(-1, "Terminal engine unavailable (GhosttyKit not loaded).")
            return
        }

        let runsDir = MacAppSupport.dir().appendingPathComponent("terminal-runs")
        try? FileManager.default.createDirectory(at: runsDir, withIntermediateDirectories: true)
        let promptPath = runsDir.appendingPathComponent("\(runId).prompt").path
        let scriptPath = runsDir.appendingPathComponent("\(runId).sh").path
        let outPath = runsDir.appendingPathComponent("\(runId).out").path
        let codePath = runsDir.appendingPathComponent("\(runId).code").path
        try? prompt.write(toFile: promptPath, atomically: true, encoding: .utf8)

        let script = interactive
            ? Self.buildInteractiveScript(program: program, argv: argv, env: env, promptPath: promptPath)
            : Self.buildScript(program: program, argv: argv, env: env,
                               promptPath: promptPath, outPath: outPath, codePath: codePath)
        try? script.write(toFile: scriptPath, atomically: true, encoding: .utf8)
        let command = "/usr/bin/env bash \(Self.shquote(scriptPath))"

        let view = MacGhosttyTerminalView(command: command, cwd: cwd)
        let title = issueIdentifier.isEmpty ? "Agent — \(program)" : "Agent — \(issueIdentifier)"

        // Interactive: mount in the dock (it owns the run lifecycle + empty submit).
        if interactive, let dock {
            dock.mountRun(runId: runId, view: view, title: title) { code, text in
                onDone(code, text)
                for p in [promptPath, scriptPath] { try? FileManager.default.removeItem(atPath: p) }
            }
            return
        }
        let session = TerminalRunSession(view: view, promptPath: promptPath, scriptPath: scriptPath,
                                      outPath: outPath, codePath: codePath, onDone: onDone)
        session.onClosed = { [weak self] in
            self?.sessions[runId] = nil
            self?.windows[runId] = nil
        }
        view.onCommandFinished = { [weak session] code in session?.finish(Int32(code)) }
        view.onProcessExit = { [weak session] in session?.finish(-1) }

        let win = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered, defer: false
        )
        win.title = title
        win.isReleasedWhenClosed = false
        win.contentView = view
        win.delegate = session
        win.center()
        win.makeKeyAndOrderFront(nil)
        win.makeFirstResponder(view)
        sessions[runId] = session
        windows[runId] = win
    }

    /// Tear down the terminal hosting `runId` (the core cancelled the run).
    /// Destroying the surface kills the CLI child; the duplicate `-1` submit from
    /// the dock/session close path is harmless — the core's channel is already
    /// resolved with the cancel sentinel.
    func terminate(runId: String) {
        if let dock, dock.currentRunId == runId {
            dock.close()
            return
        }
        windows[runId]?.close() // windowWillClose finishes + frees the session
    }

    // MARK: - Wrapper script (mirrors agent_manager.zig)

    private static func buildScript(
        program: String, argv: [String], env: [String: String],
        promptPath: String, outPath: String, codePath: String
    ) -> String {
        var s = "#!/usr/bin/env bash\nset -o pipefail\n"
        for (k, v) in env { s += "export \(k)=\(shquote(v))\n" }
        s += shquote(program)
        for a in argv { s += " " + shquote(a) }
        s += " \"$(cat \(shquote(promptPath)))\" 2>&1 | tee \(shquote(outPath))\n"
        s += "echo \"${PIPESTATUS[0]}\" > \(shquote(codePath))\n"
        return s
    }

    /// Interactive wrapper (mirrors Linux `buildInteractiveScript`): no tee /
    /// PIPESTATUS capture — the user watches and steers the CLI directly, and the
    /// plan/code is delivered out-of-band via MCP.
    private static func buildInteractiveScript(
        program: String, argv: [String], env: [String: String], promptPath: String
    ) -> String {
        var s = "#!/usr/bin/env bash\n"
        for (k, v) in env { s += "export \(k)=\(shquote(v))\n" }
        s += shquote(program)
        for a in argv { s += " " + shquote(a) }
        s += " \"$(cat \(shquote(promptPath)))\"\n"
        return s
    }

    /// Single-quote a shell token, escaping embedded single quotes as '\''.
    static func shquote(_ t: String) -> String {
        "'" + t.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}
