import Foundation

/// Host-side launcher for generic `command` run targets (masterplan §4c): the
/// play menu selects a target, the trust gate approves its `argv`/`cwd`
/// (folded into the same per-repo command-set hash the preview path uses), and
/// the process is spawned — no shell — into the bottom terminal dock via
/// `MacTerminalRunner` (falling back to a per-run window when the dock is busy
/// hosting a live coding session, which must never be evicted). Exit codes are
/// recorded per config ("last run: exit 0 · 2m ago"), and the last-selected
/// target id is persisted per repo in `last-run.json` beside the trust store —
/// host-side state only, never synced.
@MainActor
@Observable
final class MacRunConfigLauncher {
    struct RunRecord: Sendable {
        let exitCode: Int32
        let endedAt: Date
    }

    /// The run-config target currently running (one at a time — starting a new
    /// one replaces it). nil when idle.
    private(set) var activeTargetId: String?
    private var activeRunId: String?
    /// Last completed run per `"\(repo)|\(targetId)"` key (in-memory only).
    private(set) var lastRuns: [String: RunRecord] = [:]

    private let toasts: MacToastCenter
    // repo → last-selected run-config target id, persisted across launches.
    private var lastSelectedByRepo: [String: String]

    init(toasts: MacToastCenter) {
        self.toasts = toasts
        self.lastSelectedByRepo = Self.loadLastSelected()
    }

    var isRunning: Bool { activeRunId != nil }

    // MARK: - Last-selected memory (per repo, persisted)

    private static func storeURL() -> URL {
        MacAppSupport.dir().appendingPathComponent("last-run.json")
    }

    private static func loadLastSelected() -> [String: String] {
        guard let data = try? Data(contentsOf: storeURL()),
              let map = try? JSONDecoder().decode([String: String].self, from: data) else { return [:] }
        return map
    }

    func lastSelected(forRepo repo: String) -> String? {
        lastSelectedByRepo[repo]
    }

    func select(repo: String, targetId: String) {
        guard lastSelectedByRepo[repo] != targetId else { return }
        lastSelectedByRepo[repo] = targetId
        if let data = try? JSONEncoder().encode(lastSelectedByRepo) {
            try? data.write(to: Self.storeURL())
        }
    }

    // MARK: - Run history

    func lastRun(repo: String, targetId: String) -> RunRecord? {
        lastRuns["\(repo)|\(targetId)"]
    }

    // MARK: - Run / stop

    /// Spawn a trusted `command` target. Caller must have cleared the trust
    /// gate (`MacPreviewTrust.isTrusted`) — this never silently approves.
    func run(target: RunTarget, repo: String) {
        guard target.platform == .command, let argv = target.argv, !argv.isEmpty else { return }
        guard MacPreviewTrust.isTrusted(repo: repo, targets: [target]) else {
            toasts.show("Approve the run commands first.", style: .error)
            return
        }
        guard let tree = MacPreviewConfig.repoWorkingTree(forRepo: repo),
              FileManager.default.fileExists(atPath: tree.path) else {
            toasts.show("The repo isn't cloned yet — start a coding session first.", style: .error, duration: .seconds(6))
            return
        }

        // Working directory: repo-relative `cwd` wins, else the shared rootDir.
        // '..' was rejected at parse time; belt-and-braces here too.
        var dir = tree
        let sub = (target.cwd?.isEmpty == false) ? target.cwd : target.rootDir
        if let sub, !sub.isEmpty, !sub.contains("..") {
            dir = tree.appendingPathComponent(sub, isDirectory: true)
        }

        // One run-config process at a time: replace a previous one (but never a
        // coding session — see dock check below).
        if let activeRunId {
            MacTerminalRunner.shared.terminate(runId: activeRunId)
        }

        let runId = "runcfg-\(UUID().uuidString)"
        activeRunId = runId
        activeTargetId = target.id
        select(repo: repo, targetId: target.id)
        let recordKey = "\(repo)|\(target.id)"

        // Prefer the dock tab; when it's occupied (e.g. a live coding session),
        // fall back to a per-run window rather than evicting the session.
        let dockFree = !(MacTerminalRunner.shared.dock?.isMounted ?? false)
        MacTerminalRunner.shared.run(
            runId: runId,
            program: argv[0],
            argv: Array(argv.dropFirst()),
            env: PreviewShell.augmentedEnvironment(target.env),
            cwd: dir.path,
            prompt: "",
            interactive: dockFree,
            issueIdentifier: target.name
        ) { [weak self] code, _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.lastRuns[recordKey] = RunRecord(exitCode: code, endedAt: Date())
                    if self.activeRunId == runId {
                        self.activeRunId = nil
                        self.activeTargetId = nil
                    }
                }
            }
        }
    }

    /// Stop the active run-config process (tears down its terminal, which kills
    /// the child). No-op when idle.
    func stop() {
        guard let activeRunId else { return }
        MacTerminalRunner.shared.terminate(runId: activeRunId)
    }
}
