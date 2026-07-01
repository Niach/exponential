import AppKit
import SwiftUI

/// The collapsible IDE-style bottom terminal dock (roadmap D11). Hosts ONE
/// interactive agent run's libghostty terminal at a time, retained here (never by
/// SwiftUI) so it survives issue navigation. `MacShell` renders it; the
/// `MacTerminalRunner` mounts interactive runs into it. Headless background
/// runs use a per-run window instead, so they never steal the dock.
@MainActor
@Observable
final class MacTerminalDock {
    private(set) var isMounted = false
    var isExpanded = true
    var dockHeight: CGFloat = 280
    private(set) var title = ""
    /// The run currently mounted, so a core `run_cancelled` event can tear down
    /// exactly the matching terminal.
    private(set) var currentRunId: String?
    /// The owned terminal view. Retained here so the ghostty surface (which holds
    /// an unretained userdata pointer back to it) always outlives its surface.
    private(set) var terminalView: MacGhosttyTerminalView?

    private var onDone: ((Int32, String) -> Void)?
    private var submitted = false

    /// Mount an interactive run's terminal. `onDone(code, "")` is delivered to the
    /// core exactly once — on command-finished OR when the dock is closed — so the
    /// parked pipeline always unblocks. Interactive runs deliver their work via
    /// MCP, so there's nothing to capture (empty final text).
    func mountRun(
        runId: String,
        view: MacGhosttyTerminalView,
        title: String,
        onDone: @escaping (Int32, String) -> Void
    ) {
        // A previous run shouldn't still be mounted (the core runs one interactive
        // session at a time with maxConcurrent 1), but if one is, tear it down first.
        if isMounted { close() }
        self.title = title
        self.onDone = onDone
        self.submitted = false
        self.terminalView = view
        self.currentRunId = runId
        // The CLI exited → submit the result (the terminal lingers via
        // wait_after_command so the user can read the output until they close it).
        view.onCommandFinished = { [weak self] code in self?.submit(Int32(code)) }
        // The surface asked to close (the shell exited) → tear the dock down.
        view.onProcessExit = { [weak self] in self?.close() }
        view.onTitleChange = { [weak self] t in self?.title = t }
        isExpanded = true
        isMounted = true
    }

    /// Submit the run result to the core without unmounting (the terminal stays
    /// visible until the user closes the dock).
    private func submit(_ code: Int32) {
        guard !submitted else { return }
        submitted = true
        onDone?(code, "")
    }

    /// Close the dock: if the run never reported a result (the user cancelled
    /// early), submit a failure so the core's pipeline unblocks; then free the
    /// surface and hide the dock.
    func close() {
        submit(-1)
        onDone = nil
        terminalView?.destroySurface()
        terminalView = nil
        title = ""
        currentRunId = nil
        isMounted = false
    }

    func toggleExpanded() { isExpanded.toggle() }
}

/// Hosts the dock's owned `MacGhosttyTerminalView` (an NSView) inside SwiftUI. The
/// container is what SwiftUI owns/resizes; the ghostty view is added as a fill
/// subview only while mounted+expanded — and the view self-guards surface creation
/// until it has a nonzero backing size, satisfying the libghostty lazy-init rule.
struct TerminalDockHost: NSViewRepresentable {
    let dock: MacTerminalDock

    func makeNSView(context: Context) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        return container
    }

    func updateNSView(_ container: NSView, context: Context) {
        let view = dock.terminalView
        for sub in container.subviews where sub !== view {
            sub.removeFromSuperview()
        }
        guard dock.isMounted, dock.isExpanded, let view else { return }
        if view.superview !== container {
            view.frame = container.bounds
            view.autoresizingMask = [.width, .height]
            container.addSubview(view)
            container.window?.makeFirstResponder(view)
        }
    }
}
