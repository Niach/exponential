import AppKit
import ExpCore
import Foundation

/// The preview lifecycle phases (parallel to the agent run states). Drives the
/// header buttons + the pane's status copy.
enum PreviewPhase: Equatable, Sendable {
    case idle
    case doctor          // checking prerequisites
    case setup           // running the target's setup command
    case building        // compiling (android/ios)
    case booting         // starting the emulator / simulator
    case installing      // adb install / simctl install
    case launching       // am start / launch
    case running         // embedded + live
    case error(String)
    case needsMac        // iOS target on a non-Mac host (unreachable here, but modeled for parity)

    var isActive: Bool {
        switch self {
        case .idle, .running, .error, .needsMac: false
        default: true
        }
    }

    var label: String {
        switch self {
        case .idle: "Idle"
        case .doctor: "Checking prerequisites…"
        case .setup: "Installing dependencies…"
        case .building: "Building…"
        case .booting: "Starting device…"
        case .installing: "Installing app…"
        case .launching: "Launching…"
        case .running: "Running"
        case let .error(message): message
        case .needsMac: "Needs a Mac"
        }
    }
}

/// Owns ONE active preview at a time: the selected run target, the phase
/// machine, the platform backend (which owns the child processes + the embed
/// surface), and an idempotent ordered teardown. Parallel to `MacTerminalDock`:
/// `MacShell`/`MacPreviewHost` render it; the backend drives the embed.
///
/// Threading: every published mutation + every surface mount happens on the main
/// actor; the backends marshal their off-thread builds/boots/polls back here.
@MainActor
@Observable
final class MacPreviewController {
    // The repo + project the preview is scoped to (set on open / project switch).
    private(set) var repo: String?
    private(set) var projectId: String?
    private(set) var accountId: String?
    private(set) var mirror: ProjectPreviewMirror?

    /// Run targets discovered from the cloned repo file (canonical commands).
    /// Empty until the repo is cloned + parsed; the picker then falls back to
    /// the synced mirror's display-only target list (no commands → not runnable).
    private(set) var targets: [RunTarget] = []
    var selectedTargetId: String?

    private(set) var phase: PreviewPhase = .idle
    /// True while a build/run is mounted, so the host knows to show the surface.
    private(set) var isMounted = false
    /// The current backend's embed surface (NSView). Mounted by PreviewDockHost
    /// only at nonzero size (libghostty/SCStream lazy-init rule). nil when idle.
    private(set) var embedView: NSView?
    /// Annotate mode toggled from the header; the host overlays the canvas.
    var annotating = false

    private var backend: PreviewBackend?
    // Remembers the last-selected target per project (id keyed by projectId).
    private var lastSelectedByProject: [String: String] = [:]

    init() {}

    var selectedTarget: RunTarget? {
        guard let selectedTargetId else { return nil }
        return targets.first { $0.id == selectedTargetId }
    }

    /// Display targets for the picker: the repo-file targets when present, else
    /// the synced mirror's metadata (grouped by platform in the UI). `command`
    /// targets are excluded — they have no embed surface and run from the play
    /// menu instead (masterplan §4c).
    var pickerTargets: [PickerTarget] {
        if !targets.isEmpty {
            return targets
                .filter { $0.platform != .command }
                .map { PickerTarget(id: $0.id, name: $0.name, platform: $0.platform, runnable: true) }
        }
        return (mirror?.targets ?? []).compactMap { target in
            PreviewPlatform(wire: target.platform).flatMap {
                $0 == .command ? nil : PickerTarget(id: target.id, name: target.name, platform: $0, runnable: false)
            }
        }
    }

    struct PickerTarget: Identifiable, Sendable {
        let id: String
        let name: String
        let platform: PreviewPlatform
        // false when only the synced mirror is known (no commands to run yet).
        let runnable: Bool
    }

    // MARK: - Project binding

    /// Point the preview at a project. Tears down any running preview, reloads
    /// targets from the cloned repo file (if present) + the synced mirror, and
    /// restores the last-selected target for this project.
    func bind(accountId: String, project: ProjectEntity) {
        let newProjectId = project.id
        if projectId != newProjectId { stop() }
        self.accountId = accountId
        self.projectId = newProjectId
        self.repo = project.githubRepo?.isEmpty == false ? project.githubRepo : nil
        self.mirror = MacPreviewConfig.parseMirror(project.previewConfig)
        reloadTargets()
        restoreSelection()
    }

    /// Re-read the repo file (called on bind + when the user retries). Safe to
    /// call repeatedly; never auto-runs anything.
    func reloadTargets() {
        guard let repo else { targets = []; return }
        targets = MacPreviewConfig.load(forRepo: repo)?.targets ?? []
    }

    private func restoreSelection() {
        guard let projectId else { return }
        if let remembered = lastSelectedByProject[projectId],
           pickerTargets.contains(where: { $0.id == remembered }) {
            selectedTargetId = remembered
        } else {
            selectedTargetId = pickerTargets.first?.id
        }
    }

    func select(targetId: String) {
        selectedTargetId = targetId
        if let projectId { lastSelectedByProject[projectId] = targetId }
    }

    // MARK: - Trust gate

    /// Whether the selected target's command set still needs a trust prompt
    /// before its first Run. nil when there's nothing to gate (no target / repo).
    var needsTrustPrompt: Bool {
        guard let repo, let target = selectedTarget else { return false }
        return !MacPreviewTrust.isTrusted(repo: repo, targets: [target])
    }

    func approveTrust() {
        guard let repo, let target = selectedTarget else { return }
        MacPreviewTrust.approve(repo: repo, targets: [target])
    }

    // MARK: - Run / stop

    /// Build + run the selected target. Caller must have cleared the trust gate
    /// (`needsTrustPrompt` == false) — Run does not silently approve commands.
    func run() {
        guard let target = selectedTarget else {
            phase = .error("No run target selected.")
            return
        }
        guard let accountId, let repo, let workingTree = MacPreviewConfig.repoWorkingTree(forRepo: repo) else {
            phase = .error("Link a GitHub repo and let the agent clone it first.")
            return
        }
        guard MacPreviewTrust.isTrusted(repo: repo, targets: [target]) else {
            phase = .error("Approve the preview commands to run.")
            return
        }
        guard let backend = Self.makeBackend(for: target.platform) else {
            phase = .error("Command targets run from the play menu, not the preview pane.")
            return
        }
        // Single active preview: replace anything running.
        stop()

        self.backend = backend
        annotating = false
        isMounted = true

        let context = PreviewRunContext(
            target: target,
            repo: repo,
            accountId: accountId,
            workingTree: workingTree
        )
        backend.start(
            context: context,
            onPhase: { [weak self] phase in self?.phase = phase },
            onSurface: { [weak self] view in self?.mount(view) }
        )
    }

    private func mount(_ view: NSView?) {
        // Never mount at 0×0 — the host's updateNSView re-mounts at real size.
        embedView = view
    }

    /// Idempotent, ordered teardown: detach the embed → let the backend stop its
    /// children (un-reparent / stop SCStream / serve-sim --kill → graceful kill →
    /// hard kill → free well-known ports). Safe on pane close, project/target
    /// switch, app quit.
    func stop() {
        annotating = false
        embedView = nil
        isMounted = false
        if let backend {
            backend.stop()
            self.backend = nil
        }
        if case .error = phase {} else { phase = .idle }
    }

    /// Hard reset for app quit / sign-out: stop + drop project binding.
    func shutdown() {
        stop()
        repo = nil
        projectId = nil
        accountId = nil
        mirror = nil
        targets = []
        selectedTargetId = nil
    }

    // MARK: - Annotation frame

    /// Grab a clean (un-annotated) frame of what's on screen, in image-pixel
    /// space, for the annotate overlay → feedback report. Backend-specific:
    /// android = `adb exec-out screencap -p`; ios = one MJPEG JPEG frame; web =
    /// webview snapshot. Returns the decoded image or nil.
    func captureFrame() async -> CGImage? {
        await backend?.captureFrame()
    }

    /// The routing target for filed feedback: the mirror's feedbackProjectId
    /// when set, else the previewed project.
    var feedbackProjectId: String? {
        guard let projectId else { return nil }
        return mirror?.feedbackProjectId ?? projectId
    }

    /// nil for `command` targets — they have no embed surface; the play menu's
    /// `MacRunConfigLauncher` spawns them into a terminal-dock tab instead.
    private static func makeBackend(for platform: PreviewPlatform) -> PreviewBackend? {
        switch platform {
        case .web: WebPreviewBackend()
        case .android: AndroidPreviewBackend()
        case .ios: IOSSimPreviewBackend()
        case .command: nil
        }
    }
}
