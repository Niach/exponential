import ExpCore
import Foundation
import GRDB

/// The native "Start coding" launcher (masterplan §4a) — no Rust core, no FFI.
/// The sequence, driven by a single `start(accountId:issueId:)` entry point (so
/// the Phase 4 relay's `start_session` can trigger the exact same path):
///
/// 1. Resolve the repo — `repositories.forIssue` (null ⇒ "Link a repository").
/// 2. Mint a JIT push token — `repositories.installationToken`.
/// 3. Host-side git — clone/fetch + worktree + `exp/<IDENTIFIER>` branch + a
///    token-embedded `origin` remote (via `GitWorktree`, never `gh`).
/// 4. Write `.mcp.json` (web `/api/mcp` + personal API key) and a plan-first
///    `PROMPT.md` into the worktree.
/// 5. Open a `coding_sessions` row (`running`).
/// 6. Spawn `claude --dangerously-skip-permissions` (cwd = worktree) in the
///    embedded ghostty terminal; end the session when the run closes.
@MainActor
@Observable
final class MacCodingLauncher {
    /// Issues with a live coding session on this Mac (drives the play button's
    /// running indicator).
    private(set) var runningIssueIds: Set<String> = []
    /// issueId → coding_sessions.id, so the completion handler ends the right row.
    private var sessionByIssue: [String: String] = [:]

    private let auth: AuthRepository
    private let repositoriesApi: RepositoriesApi
    private let codingSessionsApi: CodingSessionsApi
    private let steerApi: SteerApi
    private let db: DatabaseManager
    private let settings: MacCodingSettings
    private let toasts: MacToastCenter
    private let terminalDock: MacTerminalDock

    // Live steer publishers + PTY tails, keyed by issueId (torn down on finish).
    private var publishers: [String: MacSteerPublisher] = [:]
    private var tails: [String: MacSteerPtyTail] = [:]
    private var rawFiles: [String: URL] = [:]

    init(
        auth: AuthRepository,
        repositoriesApi: RepositoriesApi,
        codingSessionsApi: CodingSessionsApi,
        steerApi: SteerApi,
        db: DatabaseManager,
        settings: MacCodingSettings,
        toasts: MacToastCenter,
        terminalDock: MacTerminalDock
    ) {
        self.auth = auth
        self.repositoriesApi = repositoriesApi
        self.codingSessionsApi = codingSessionsApi
        self.steerApi = steerApi
        self.db = db
        self.settings = settings
        self.toasts = toasts
        self.terminalDock = terminalDock
    }

    func isRunning(issueId: String) -> Bool { runningIssueIds.contains(issueId) }

    /// The single entry point — local play button AND the Phase-4 relay
    /// `start_session` command call this. No-op if a session is already live.
    func start(accountId: String, issueId: String) {
        guard !runningIssueIds.contains(issueId) else { return }
        Task { await run(accountId: accountId, issueId: issueId) }
    }

    // The blocking git + file prep runs off the main actor; this Sendable result
    // carries either the worktree path or a human-readable failure.
    private enum PrepResult: Sendable {
        case ok(URL)
        case failed(String)
    }

    private func run(accountId: String, issueId: String) async {
        // Read the synced issue locally for the prompt (identifier/title/body).
        guard let pool = try? db.pool(forAccountId: accountId),
              let issue = try? await pool.read({ db in try IssueEntity.fetchOne(db, key: issueId) })
        else {
            toasts.show("Couldn't load that issue.", style: .error)
            return
        }

        // 1. Resolve the repo (coding-first gate).
        let repo: RepoForIssue?
        do {
            repo = try await repositoriesApi.forIssue(accountId: accountId, issueId: issueId)
        } catch {
            toasts.show("Couldn't resolve the repository: \(error.localizedDescription)", style: .error)
            return
        }
        guard let repo else {
            toasts.show(
                "Link a repository to this project in workspace settings to start coding.",
                style: .error, duration: .seconds(6))
            return
        }

        // Personal API key (written into .mcp.json so claude auths as the user).
        guard settings.hasPersonalKey, let personalKey = settings.personalApiKey else {
            toasts.show(
                "Generate a personal API key in Settings → Coding first.",
                style: .error, duration: .seconds(6))
            return
        }
        guard let baseUrl = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl else {
            toasts.show("No instance URL for this account.", style: .error)
            return
        }

        // 2. Mint a short-lived push token.
        let tok: InstallationToken
        do {
            tok = try await repositoriesApi.installationToken(
                accountId: accountId, repositoryId: repo.repositoryId)
        } catch {
            toasts.show(
                "Couldn't mint a push token — reconnect this repo. \(error.localizedDescription)",
                style: .error, duration: .seconds(6))
            return
        }

        let identifier = issue.identifier ?? issueId
        let branch = settings.branchPrefix + identifier
        // The token is embedded in the remote URL — never log this string.
        let tokenUrl = "https://x-access-token:\(tok.token)@github.com/\(tok.fullName).git"
        let reposRoot = settings.reposRootURL
        let baseRef = "origin/\(tok.defaultBranch)"
        let promptBody = Self.composePrompt(
            identifier: identifier, title: issue.title,
            description: getIssueDescriptionText(issue.description), branch: branch)

        // 3–4. Host-side git + worktree files (blocking → off the main actor).
        let prepared: PrepResult = await Task.detached(priority: .userInitiated) {
            do {
                let clonePath = try GitWorktree.ensureClone(
                    reposRoot: reposRoot, fullName: tok.fullName, tokenUrl: tokenUrl)
                let worktree = try GitWorktree.createWorktree(
                    clonePath: clonePath, branch: branch, baseRef: baseRef)
                try GitWorktree.setTokenRemote(worktreePath: worktree, tokenUrl: tokenUrl)
                try Self.writeMcpJson(worktree: worktree, baseUrl: baseUrl, personalKey: personalKey)
                try promptBody.write(
                    to: worktree.appendingPathComponent("PROMPT.md"),
                    atomically: true, encoding: .utf8)
                return .ok(worktree)
            } catch {
                return .failed(error.localizedDescription)
            }
        }.value

        let worktree: URL
        switch prepared {
        case .ok(let w): worktree = w
        case .failed(let detail):
            toasts.show("Git setup failed: \(detail)", style: .error, duration: .seconds(8))
            return
        }

        // 5. Open the coding_sessions row.
        let session: CodingSessionRef
        do {
            session = try await codingSessionsApi.start(
                accountId: accountId, issueId: issueId, deviceLabel: Self.deviceLabel)
        } catch {
            toasts.show("Couldn't start the coding session: \(error.localizedDescription)", style: .error)
            return
        }
        runningIssueIds.insert(issueId)
        sessionByIssue[issueId] = session.id

        // Steer (optional, purely additive): if the relay is configured on this
        // instance, tee this session's PTY to it for remote watch/steer. libghostty
        // owns the child PTY (no host-side output-read seam), so when steering is on
        // we run `claude` under `script(1)` — it gives claude a real PTY AND tees
        // the verbatim terminal bytes to a file the publisher tails. Disabled ⇒ the
        // plain spawn (byte-identical to local-only coding).
        let steerEnabled = ((try? await steerApi.config(accountId: accountId))?.enabled) ?? false
        let program: String
        let argv: [String]
        var rawFile: URL?
        if steerEnabled {
            let file = Self.steerRawFile(sessionId: session.id)
            rawFile = file
            program = "/usr/bin/script"
            argv = ["-q", file.path, settings.claudePath, "--dangerously-skip-permissions"]
        } else {
            program = settings.claudePath
            argv = ["--dangerously-skip-permissions"]
        }

        // 6. Spawn `claude` in the embedded ghostty terminal (dock), keyed by the
        //    coding_sessions.id. Completion ends the session.
        MacTerminalRunner.shared.run(
            runId: session.id,
            program: program,
            argv: argv,
            env: PreviewShell.augmentedEnvironment(),
            cwd: worktree.path,
            prompt: "Read PROMPT.md in this directory, then follow it.",
            interactive: true,
            issueIdentifier: identifier
        ) { [weak self] _, _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.finish(accountId: accountId, issueId: issueId) }
            }
        }

        // Attach the steer publisher (data-plane) for this session.
        if steerEnabled, let rawFile {
            attachSteer(sessionId: session.id, issueId: issueId, accountId: accountId, rawFile: rawFile)
        }

        toasts.show(
            "Coding session started for \(identifier) — watch it in the terminal.",
            style: .success)
    }

    // MARK: - Steer publisher attach/detach (masterplan §3.3)

    private func attachSteer(sessionId: String, issueId: String, accountId: String, rawFile: URL) {
        let dock = terminalDock
        let publisher = MacSteerPublisher(
            sessionId: sessionId,
            issueId: issueId,
            accountId: accountId,
            steerApi: steerApi,
            inputSink: { [weak dock] text in
                // Inject remote keystrokes only into THIS session's live terminal.
                if dock?.currentRunId == sessionId { dock?.terminalView?.writeToPty(text) }
            },
            onKill: { MacTerminalRunner.shared.terminate(runId: sessionId) }
        )
        let tail = MacSteerPtyTail(path: rawFile.path) { [weak publisher] data in
            Task { @MainActor in publisher?.feed(data) }
        }
        publishers[issueId] = publisher
        tails[issueId] = tail
        rawFiles[issueId] = rawFile
        publisher.start()
        tail.start()
    }

    /// Per-session raw PTY-tee file under the app-support dir.
    static func steerRawFile(sessionId: String) -> URL {
        let dir = MacAppSupport.dir().appendingPathComponent("steer", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("\(sessionId).raw")
    }

    /// End the coding_sessions row + clear the running flag when the terminal
    /// session closes. Idempotent (the dock may report completion once).
    private func finish(accountId: String, issueId: String) {
        runningIssueIds.remove(issueId)
        // Tear the steer publisher + PTY tail down and drop the raw tee file.
        publishers[issueId]?.stop(outcome: "ended")
        publishers[issueId] = nil
        tails[issueId]?.stop()
        tails[issueId] = nil
        if let raw = rawFiles.removeValue(forKey: issueId) {
            try? FileManager.default.removeItem(at: raw)
        }
        guard let sessionId = sessionByIssue.removeValue(forKey: issueId) else { return }
        let api = codingSessionsApi
        Task { try? await api.end(accountId: accountId, id: sessionId) }
    }

    // MARK: - Helpers

    nonisolated static var deviceLabel: String { ProcessInfo.processInfo.hostName }

    /// Write the worktree `.mcp.json` pointing `claude` at the web MCP server,
    /// authenticated with the user's personal API key.
    nonisolated private static func writeMcpJson(worktree: URL, baseUrl: String, personalKey: String) throws {
        let config: [String: Any] = [
            "mcpServers": [
                "exponential": [
                    "type": "http",
                    "url": "\(baseUrl)/api/mcp",
                    "headers": ["Authorization": "Bearer \(personalKey)"],
                ],
            ],
        ]
        let data = try JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted])
        try data.write(to: worktree.appendingPathComponent(".mcp.json"))
    }

    /// The plan-first prompt written to `PROMPT.md`. `claude` is launched with
    /// "Read PROMPT.md in this directory, then follow it."
    nonisolated private static func composePrompt(
        identifier: String, title: String, description: String, branch: String
    ) -> String {
        """
        # \(identifier): \(title)

        \(description.isEmpty ? "_No description provided._" : description)

        ---

        You are working on the issue above in a fresh git worktree on branch `\(branch)`.

        1. First propose a concise implementation plan and WAIT for my go-ahead — do not write code yet.
        2. After I approve the plan, implement the change.
        3. When you're done: commit your work, push the branch `\(branch)`, then call the `exponential_pr_open` MCP tool to open a pull request linked to this issue.
        4. You may move the issue forward with the `exponential_issues_update_status` MCP tool.
        """
    }
}
