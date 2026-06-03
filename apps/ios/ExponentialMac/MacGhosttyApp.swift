import AppKit
import Foundation
import GhosttyKit
import os

private let log = Logger(subsystem: "com.straehhuber.exponential.mac", category: "Ghostty")

/// Process-global libghostty app: init, config, the 60Hz tick, runtime callbacks.
/// Mirrors github.com/thdxg/macterm's GhosttyApp (the macOS reference for the
/// embedded apprt). Created lazily on first agent-run terminal.
@MainActor
final class MacGhosttyApp {
    static let shared = MacGhosttyApp()

    private(set) var app: ghostty_app_t?
    private var config: ghostty_config_t?
    private var tickTimer: Timer?

    private init() {
        resolveResources()
        guard ghostty_init(UInt(CommandLine.argc), CommandLine.unsafeArgv) == GHOSTTY_SUCCESS else {
            log.error("ghostty_init failed")
            return
        }
        guard let cfg = ghostty_config_new() else {
            log.error("ghostty_config_new failed")
            return
        }
        ghostty_config_load_recursive_files(cfg)
        ghostty_config_finalize(cfg)
        config = cfg

        var rt = ghostty_runtime_config_s()
        rt.supports_selection_clipboard = true
        rt.wakeup_cb = { _ in DispatchQueue.main.async { MacGhosttyApp.shared.tick() } }
        rt.action_cb = { _, target, action in MacGhosttyCallbacks.action(target: target, action: action) }
        rt.read_clipboard_cb = { ud, loc, state in MacGhosttyCallbacks.readClipboard(ud: ud, location: loc, state: state) }
        rt.confirm_read_clipboard_cb = { ud, content, state, _ in MacGhosttyCallbacks.confirmReadClipboard(ud: ud, content: content, state: state) }
        rt.write_clipboard_cb = { _, _, content, len, _ in MacGhosttyCallbacks.writeClipboard(content: content, len: UInt(len)) }
        rt.close_surface_cb = { ud, _ in MacGhosttyCallbacks.closeSurface(ud: ud) }

        guard let a = ghostty_app_new(&rt, cfg) else {
            log.error("ghostty_app_new failed")
            ghostty_config_free(cfg)
            return
        }
        app = a

        let timer = Timer(timeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        RunLoop.main.add(timer, forMode: .common)
        tickTimer = timer
    }

    func tick() {
        guard let app else { return }
        ghostty_app_tick(app)
    }

    /// Point libghostty at the bundled resources (themes/shell-integration); it
    /// derives TERMINFO as the sibling Resources/terminfo. Fetched by
    /// scripts/setup-ghostty-macos.sh into the app bundle.
    private func resolveResources() {
        guard let res = Bundle.main.resourceURL?.appendingPathComponent("ghostty").path,
              FileManager.default.fileExists(atPath: res) else { return }
        setenv("GHOSTTY_RESOURCES_DIR", res, 1)
    }
}

/// libghostty runtime callbacks (C function pointers → no captures). Route to the
/// owning terminal view via the surface userdata.
enum MacGhosttyCallbacks {
    static func action(target: ghostty_target_s, action: ghostty_action_s) -> Bool {
        switch action.tag {
        case GHOSTTY_ACTION_COMMAND_FINISHED:
            guard let view = view(from: target) else { return true }
            let code = action.action.command_finished.exit_code
            DispatchQueue.main.async { view.onCommandFinished?(code) }
            return true
        case GHOSTTY_ACTION_SET_TITLE:
            guard let view = view(from: target), let ptr = action.action.set_title.title else { return true }
            let title = String(cString: ptr)
            DispatchQueue.main.async { view.onTitleChange?(title) }
            return true
        default:
            return false
        }
    }

    static func closeSurface(ud: UnsafeMutableRawPointer?) {
        guard let ud else { return }
        let view = Unmanaged<MacGhosttyTerminalView>.fromOpaque(ud).takeUnretainedValue()
        DispatchQueue.main.async { view.onProcessExit?() }
    }

    static func readClipboard(ud: UnsafeMutableRawPointer?, location: ghostty_clipboard_e, state: UnsafeMutableRawPointer?) -> Bool {
        let text = NSPasteboard.general.string(forType: .string) ?? ""
        text.withCString { ghostty_surface_complete_clipboard_request(surface(from: ud), $0, state, false) }
        return true
    }

    static func confirmReadClipboard(ud: UnsafeMutableRawPointer?, content: UnsafePointer<CChar>?, state: UnsafeMutableRawPointer?) {
        guard let content else { return }
        ghostty_surface_complete_clipboard_request(surface(from: ud), content, state, true)
    }

    static func writeClipboard(content: UnsafePointer<ghostty_clipboard_content_s>?, len: UInt) {
        guard let content, len > 0 else { return }
        for item in UnsafeBufferPointer(start: content, count: Int(len)) {
            guard let data = item.data, let mime = item.mime, String(cString: mime).hasPrefix("text/plain") else { continue }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(String(cString: data), forType: .string)
            return
        }
    }

    private static func view(from target: ghostty_target_s) -> MacGhosttyTerminalView? {
        guard target.tag == GHOSTTY_TARGET_SURFACE,
              let surface = target.target.surface,
              let ud = ghostty_surface_userdata(surface) else { return nil }
        return Unmanaged<MacGhosttyTerminalView>.fromOpaque(ud).takeUnretainedValue()
    }

    private static func surface(from ud: UnsafeMutableRawPointer?) -> ghostty_surface_t? {
        guard let ud else { return nil }
        return Unmanaged<MacGhosttyTerminalView>.fromOpaque(ud).takeUnretainedValue().surface
    }
}
