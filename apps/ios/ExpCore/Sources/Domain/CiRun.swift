import Foundation

/// #EXP-1617 / #EXP-1804 (packages/domain-contract fixtures/ci-run.json and
/// fixtures/runner-settings.json are the spec; TS reference `src/ci-run.ts`,
/// desktop `domain::ci_run`, Android `CiRun`): the pure half of the CI run
/// page and of a device's CI runner section, replayed by `CiRunTests`.
public enum CiRun {
    // MARK: - Copy (fixture `copy` / `testIds`)

    public enum Copy {
        public static let title = "Run #{number}"
        public static let jobs = "Jobs"
        public static let steps = "Steps"
        public static let log = "Log"
        public static let rerunFailed = "Re-run failed"
        public static let cancel = "Cancel"
        public static let emptyLog = "No log yet."
        public static let failedJobs = "{count} failed"
        public static let pipelineError = "The pipeline file did not plan"
        public static let none = "—"
        public static let tileStatus = "Status"
        public static let tileDuration = "Duration"
        public static let tileRunner = "Runner"
        public static let tileCommit = "Commit"
        public static let reasons: [String: String] = [
            "fork": "Not run: a pull request from a fork",
            "paths": "Skipped: no changed path matches",
            "needs": "Skipped: a job it needs did not pass",
            "runner_lost": "The runner stopped answering",
            "reused": "Kept from the run it re-runs",
        ]

        public static func title(_ number: Int) -> String {
            title.replacingOccurrences(of: "{number}", with: "\(number)")
        }
    }

    public enum TestIds {
        public static let page = "ci-run"
        public static let tiles = "ci-run-tiles"
        public static let jobRow = "ci-run-job"
        public static let steps = "ci-run-steps"
        public static let log = "ci-run-log"
        public static let rerun = "ci-run-rerun"
        public static let cancel = "ci-run-cancel"
    }

    public static let liveTailMs = 2000
    public static let logMaxLinesWide = 32
    public static let logMaxLinesPhone = 20

    // MARK: - Rows

    public struct Run: Equatable, Sendable {
        public let id: String
        public let number: Int
        public let name: String
        public let state: String
        public let error: String?
        public let sha: String
        public let startedAt: String?
        public let completedAt: String?

        public init(id: String, number: Int, name: String, state: String, error: String?, sha: String, startedAt: String?, completedAt: String?) {
            self.id = id
            self.number = number
            self.name = name
            self.state = state
            self.error = error
            self.sha = sha
            self.startedAt = startedAt
            self.completedAt = completedAt
        }
    }

    /// One step of a job (`ci_jobs.steps`, epoch ms).
    public struct Step: Equatable, Sendable {
        public let name: String
        public let status: String
        public let conclusion: String?
        public let startedAt: Double?
        public let completedAt: Double?

        public init(name: String, status: String, conclusion: String?, startedAt: Double?, completedAt: Double?) {
            self.name = name
            self.status = status
            self.conclusion = conclusion
            self.startedAt = startedAt
            self.completedAt = completedAt
        }
    }

    public struct Job: Equatable, Sendable {
        public let id: String
        public let check: String
        public let position: Int
        public let status: String
        public let conclusion: String?
        public let reason: String?
        public let runnerLabel: String?
        public let startedAt: String?
        public let completedAt: String?
        public let steps: [Step]

        public init(
            id: String, check: String, position: Int, status: String, conclusion: String?, reason: String? = nil,
            runnerLabel: String? = nil, startedAt: String? = nil, completedAt: String? = nil, steps: [Step] = []
        ) {
            self.id = id
            self.check = check
            self.position = position
            self.status = status
            self.conclusion = conclusion
            self.reason = reason
            self.runnerLabel = runnerLabel
            self.startedAt = startedAt
            self.completedAt = completedAt
            self.steps = steps
        }

        var asCheck: PrChecks.Check {
            PrChecks.Check(name: check, status: status, conclusion: conclusion, startedAt: startedAt, completedAt: completedAt)
        }
    }

    public enum Status: String, Sendable { case queued, running, passed, failed, cancelled }
    public enum Tone: String, Sendable { case neutral, primary, success, danger }
    public enum TraceStatus: String, Sendable { case pending, running, done, skipped, error }

    public struct Tile: Equatable, Sendable {
        public let label: String
        public let value: String
        public let tone: Tone
        /// The Status tile of a failed run: `{count} failed` (red); else nil.
        public var delta: String? = nil
    }

    public struct JobRow: Equatable, Sendable {
        public let id: String
        public let title: String
        public let subtitle: String?
        public let glyph: PrChecks.Glyph
        public let tone: PrChecks.Tone
        public let duration: String?
    }

    public struct TraceItem: Equatable, Sendable {
        public let title: String
        public let status: TraceStatus
        public let description: String?
    }

    public struct LogView: Equatable, Sendable {
        public let code: String
        public let highlight: [Int]
    }

    public struct Actions: Equatable, Sendable {
        public let rerunFailed: Bool
        public let cancel: Bool
    }

    // MARK: - Rules

    public static func isFailing(status: String, conclusion: String?) -> Bool {
        PrChecks.isFailing(status: status, conclusion: conclusion)
    }

    public static func statusLabel(_ status: Status) -> String {
        switch status {
        case .queued: "Queued"
        case .running: "Running"
        case .passed: "Passed"
        case .failed: "Failed"
        case .cancelled: "Cancelled"
        }
    }

    private static func tone(_ status: Status) -> Tone {
        switch status {
        case .queued, .cancelled: .neutral
        case .running: .primary
        case .passed: .success
        case .failed: .danger
        }
    }

    /// The run's status for its first tile.
    public static func runStatus(state: String, jobs: [(status: String, conclusion: String?)]) -> (status: Status, tone: Tone) {
        let status: Status
        if state == "pending" {
            status = jobs.contains { $0.status == "in_progress" } ? .running : .queued
        } else if state == "success" {
            status = .passed
        } else {
            let failing = jobs.filter { isFailing(status: $0.status, conclusion: $0.conclusion) }
            status = !failing.isEmpty && failing.allSatisfy { $0.conclusion == "cancelled" } ? .cancelled : .failed
        }
        return (status, tone(status))
    }

    static func ms(_ value: String?) -> Double? {
        guard let value, !value.isEmpty, let date = WireTimestamps.parse(value) else { return nil }
        return date.timeIntervalSince1970 * 1000
    }

    /// Status, Duration, Runner, Commit.
    public static func tiles(
        state: String, sha: String, startedAt: String?, completedAt: String?,
        jobs: [(status: String, conclusion: String?, runnerLabel: String?)], now: Date
    ) -> [Tile] {
        let (status, tone) = runStatus(state: state, jobs: jobs.map { ($0.status, $0.conclusion) })
        let end = ms(completedAt) ?? now.timeIntervalSince1970 * 1000
        var runners: [String] = []
        for job in jobs {
            if let label = job.runnerLabel, !label.isEmpty, !runners.contains(label) { runners.append(label) }
        }
        let failing = jobs.filter { isFailing(status: $0.status, conclusion: $0.conclusion) }.count
        let delta = status == .failed && failing > 0 ? Copy.failedJobs.replacingOccurrences(of: "{count}", with: String(failing)) : nil
        return [
            Tile(label: Copy.tileStatus, value: statusLabel(status), tone: tone, delta: delta),
            Tile(
                label: Copy.tileDuration,
                value: ms(startedAt).map { PrChecks.formatDuration(ms: end - $0) } ?? Copy.none,
                tone: .neutral
            ),
            Tile(label: Copy.tileRunner, value: runners.isEmpty ? Copy.none : runners.joined(separator: ", "), tone: .neutral),
            Tile(label: Copy.tileCommit, value: String(sha.prefix(7)), tone: .neutral),
        ]
    }

    public static func tiles(_ run: Run, jobs: [Job], now: Date) -> [Tile] {
        tiles(
            state: run.state, sha: run.sha, startedAt: run.startedAt, completedAt: run.completedAt,
            jobs: jobs.map { ($0.status, $0.conclusion, $0.runnerLabel) }, now: now
        )
    }

    private static func byPosition(_ jobs: [Job]) -> [Job] {
        jobs.enumerated().sorted { a, b in
            a.element.position != b.element.position ? a.element.position < b.element.position : a.offset < b.offset
        }.map(\.element)
    }

    private static func reasonText(_ reason: String?) -> String? {
        reason.flatMap { Copy.reasons[$0] }
    }

    /// The jobs in planner order.
    public static func jobRows(_ jobs: [Job], now: Date) -> [JobRow] {
        byPosition(jobs).map { job in
            let (glyph, tone) = PrChecks.checkGlyph(status: job.status, conclusion: job.conclusion)
            return JobRow(
                id: job.id, title: job.check,
                subtitle: job.status == "completed" ? reasonText(job.reason) : nil,
                glyph: glyph, tone: tone,
                duration: PrChecks.checkDuration(job.asCheck, now: now)
            )
        }
    }

    /// The selected job: the requested one, else the first failing, running, any.
    public static func defaultJob(_ jobs: [Job], requested: String?) -> String? {
        let ordered = byPosition(jobs)
        if let requested, !requested.isEmpty, ordered.contains(where: { $0.id == requested }) { return requested }
        return (ordered.first { isFailing(status: $0.status, conclusion: $0.conclusion) }
            ?? ordered.first { $0.status == "in_progress" }
            ?? ordered.first)?.id
    }

    private static func traceStatus(_ step: Step) -> TraceStatus {
        if step.status == "queued" { return .pending }
        if step.status != "completed" { return .running }
        if step.conclusion == "success" { return .done }
        if step.conclusion == "skipped" || step.conclusion == "neutral" { return .skipped }
        return .error
    }

    /// A job's steps as trace items.
    public static func steps(_ steps: [Step], now: Date) -> [TraceItem] {
        let at = now.timeIntervalSince1970 * 1000
        return steps.map { step in
            let end: Double? = step.status == "completed" ? step.completedAt : step.status == "in_progress" ? at : nil
            var description: String?
            if let started = step.startedAt, let end { description = PrChecks.formatDuration(ms: end - started) }
            return TraceItem(title: step.name, status: traceStatus(step), description: description)
        }
    }

    /// The log as shown: the runner's markers become plain lines; error lines
    /// are highlighted (1-based, displayed lines).
    public static func logView(_ text: String) -> LogView {
        var lines = text.components(separatedBy: "\n")
        if lines.last == "" { lines.removeLast() }
        var out: [String] = []
        var highlight: [Int] = []
        for line in lines {
            if line.hasPrefix("##[endgroup]") { continue }
            if line.hasPrefix("##[group]") {
                out.append("▸ " + line.dropFirst("##[group]".count))
            } else if line.hasPrefix("##[error]") {
                out.append("Error: " + line.dropFirst("##[error]".count))
                highlight.append(out.count)
            } else if line.hasPrefix("##[warning]") {
                out.append("Warning: " + line.dropFirst("##[warning]".count))
            } else {
                out.append(line)
            }
        }
        return LogView(code: out.joined(separator: "\n"), highlight: highlight)
    }

    /// Which buttons the run shows.
    public static func actions(state: String, error: String?) -> Actions {
        Actions(rerunFailed: state == "failure", cancel: state == "pending")
    }

    /// `/t/<team>/ci/<runId>` (query + hash ignored) → (team, runId); nil =
    /// another path.
    public static func parsePath(_ path: String) -> (team: String, runId: String)? {
        let bare = path.prefix { $0 != "#" }.prefix { $0 != "?" }
        let parts = bare.split(separator: "/").map(String.init)
        guard parts.count == 4, parts[0] == "t", parts[2] == "ci" else { return nil }
        return (parts[1], parts[3])
    }

    /// The `job` query parameter of a run link, nil when absent.
    public static func jobParam(_ path: String) -> String? {
        let noHash = path.prefix { $0 != "#" }
        guard let q = noHash.firstIndex(of: "?") else { return nil }
        let query = noHash[noHash.index(after: q)...]
        let value = query.split(separator: "&").first { $0.hasPrefix("job=") }.map { String($0.dropFirst(4)) }
        return value.flatMap { $0.isEmpty ? nil : ($0.removingPercentEncoding ?? $0) }
    }

    /// `href` is a run page of OUR CI on this instance (`origin`): a check's
    /// `details_url`, so it opens in the app.
    public static func isRunLink(_ href: String, origin: String) -> Bool {
        if case let .app(path) = AppLink.classify(href, origin: origin) { return parsePath(path) != nil }
        return false
    }

    /// The `ci.log` tail state (web `CiLogState`).
    public struct LogTail: Equatable, Sendable {
        public var jobId: String?
        public var text = ""
        public var offset = 0
        public var complete = false
        public var loaded = false

        public init(jobId: String? = nil) { self.jobId = jobId }

        /// One read appended; a chunk for another job or an older offset (a
        /// late reply) is dropped.
        public func appending(jobId: String, readFrom: Int, text chunk: String, nextOffset: Int, complete: Bool) -> LogTail {
            guard self.jobId == jobId, readFrom == offset else { return self }
            var next = self
            next.text += chunk
            next.offset = nextOffset
            next.complete = complete
            next.loaded = true
            return next
        }
    }
}

/// #EXP-1804 (fixtures/runner-settings.json): a device's CI runner section.
public enum RunnerSettings {
    public enum Copy {
        public static let section = "CI runner"
        public static let enabledLabel = "Run CI jobs"
        public static let enabledDescription = "Runs your team's pipelines on this device."
        public static let labelsLabel = "Labels"
        public static let labelsPlaceholder = "gpu, big"
        public static let labelsInvalid = "Use lowercase letters, digits, dots, dashes or underscores."
        public static let slots = "Parallel jobs"
        public static let timeout = "Job timeout"
        public static let recent = "Recent jobs"
        public static let noJobs = "No jobs yet."
        public static let runSubtitle = "Run #{number} · {repo}"
    }

    public struct Notice: Equatable, Sendable {
        public let type: String
        public let title: String
        public let message: String
    }

    public static let notices: [String: Notice] = [
        "phone": Notice(type: "info", title: "Set up here, runs on the device", message: "A phone never runs CI jobs: this sets up the device's runner."),
        "update": Notice(type: "warning", title: "Update this device", message: "Its app is too old to run CI jobs."),
    ]

    public enum TestIds {
        public static let section = "runner-settings"
        public static let toggle = "runner-enabled"
        public static let labels = "runner-labels"
        public static let slots = "runner-slots"
        public static let timeout = "runner-timeout"
        public static let recent = "runner-recent"
    }

    public static let maxLabels = 16
    public static let minSlots = 1
    public static let maxSlots = 16
    public static let maxRecent = 5
    public static let timeoutOptions: [(value: String, label: String)] = [
        ("15", "15 min"), ("30", "30 min"), ("60", "1 h"), ("120", "2 h"), ("240", "4 h"), ("360", "6 h"),
    ]

    public struct Settings: Equatable, Sendable {
        public var enabled: Bool
        public var labels: [String]
        public var slots: Int
        public var timeoutMin: Int

        public init(enabled: Bool, labels: [String], slots: Int, timeoutMin: Int) {
            self.enabled = enabled
            self.labels = labels
            self.slots = slots
            self.timeoutMin = timeoutMin
        }

        public static let defaults = Settings(enabled: false, labels: [], slots: 1, timeoutMin: 60)

        /// The row's `runner` jsonb, `defaults` filling a null one (or a
        /// missing key).
        public init(json: String?) {
            let object = json.flatMap { $0.data(using: .utf8) }
                .flatMap { try? JSONSerialization.jsonObject(with: $0) } as? [String: Any]
            let d = Settings.defaults
            enabled = (object?["enabled"] as? Bool) ?? d.enabled
            labels = (object?["labels"] as? [String]) ?? d.labels
            slots = (object?["slots"] as? NSNumber)?.intValue ?? d.slots
            timeoutMin = (object?["timeoutMin"] as? NSNumber)?.intValue ?? d.timeoutMin
        }
    }

    /// A typed label as stored, nil = refused.
    public static func normalizeLabel(_ input: String) -> String? {
        let label = input.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard label.range(of: "^[a-z0-9][a-z0-9._-]{0,31}$", options: .regularExpression) != nil else { return nil }
        return label
    }

    private static let phones: Set<String> = ["ios", "android", "web-mobile"]

    /// The section's notice key: phones never run jobs; an old app cannot.
    public static func notice(client: String, caps: [String]) -> String? {
        if phones.contains(client) { return "phone" }
        if !caps.contains("ci-runner") { return "update" }
        return nil
    }

    /// The OS label a runner always has.
    public static func osLabel(_ platform: String?) -> String? {
        switch platform {
        case "linux": "linux"
        case "macos", "darwin": "mac"
        case "windows": "windows"
        default: nil
        }
    }

    public struct Job: Equatable, Sendable {
        public let id: String
        public let runnerDeviceId: String?
        public let check: String
        public let runNumber: Int
        public let repo: String
        public let status: String
        public let conclusion: String?
        public let startedAt: String?
        public let completedAt: String?

        public init(
            id: String, runnerDeviceId: String?, check: String, runNumber: Int, repo: String,
            status: String, conclusion: String?, startedAt: String?, completedAt: String?
        ) {
            self.id = id
            self.runnerDeviceId = runnerDeviceId
            self.check = check
            self.runNumber = runNumber
            self.repo = repo
            self.status = status
            self.conclusion = conclusion
            self.startedAt = startedAt
            self.completedAt = completedAt
        }
    }

    /// The runner's newest jobs.
    public static func recentJobs(deviceId: String, jobs: [Job], now: Date) -> [CiRun.JobRow] {
        jobs.enumerated()
            .filter { $0.element.runnerDeviceId == deviceId }
            .sorted { a, b in
                let am = CiRun.ms(a.element.startedAt) ?? -.infinity
                let bm = CiRun.ms(b.element.startedAt) ?? -.infinity
                return am != bm ? am > bm : a.offset < b.offset
            }
            .prefix(maxRecent)
            .map { entry in
                let job = entry.element
                let (glyph, tone) = PrChecks.checkGlyph(status: job.status, conclusion: job.conclusion)
                let check = PrChecks.Check(
                    name: job.check, status: job.status, conclusion: job.conclusion,
                    startedAt: job.startedAt, completedAt: job.completedAt
                )
                return CiRun.JobRow(
                    id: job.id, title: job.check,
                    subtitle: Copy.runSubtitle
                        .replacingOccurrences(of: "{number}", with: "\(job.runNumber)")
                        .replacingOccurrences(of: "{repo}", with: job.repo),
                    glyph: glyph, tone: tone,
                    duration: PrChecks.checkDuration(check, now: now)
                )
            }
    }

    // MARK: - The specimen (fixture `specimen` + `recentJobs[0]`)

    public static let specimenClient = "desktop"
    public static let specimenDeviceId = "d1"
    public static let specimenPlatform = "linux"
    public static let specimenCaps = ["ci-runner"]
    public static let specimenSettings = Settings(enabled: true, labels: ["gpu"], slots: 2, timeoutMin: 60)
    public static let specimenNow = "2026-11-13T10:30:00.000Z"
    public static let specimenJobs: [Job] = [
        Job(id: "a", runnerDeviceId: "d1", check: "build", runNumber: 10, repo: "niach/exponential", status: "completed", conclusion: "success", startedAt: "2026-11-13T09:00:00.000Z", completedAt: "2026-11-13T09:02:00.000Z"),
        Job(id: "b", runnerDeviceId: "d2", check: "build", runNumber: 11, repo: "niach/exponential", status: "completed", conclusion: "success", startedAt: "2026-11-13T09:10:00.000Z", completedAt: "2026-11-13T09:12:00.000Z"),
        Job(id: "c", runnerDeviceId: "d1", check: "test (linux)", runNumber: 12, repo: "niach/exponential", status: "in_progress", conclusion: nil, startedAt: "2026-11-13T10:20:00.000Z", completedAt: nil),
        Job(id: "d", runnerDeviceId: "d1", check: "lint", runNumber: 12, repo: "niach/exponential", status: "completed", conclusion: "failure", startedAt: "2026-11-13T10:19:00.000Z", completedAt: "2026-11-13T10:19:45.000Z"),
        Job(id: "e", runnerDeviceId: "d1", check: "e2e", runNumber: 9, repo: "niach/spezl", status: "completed", conclusion: "success", startedAt: "2026-11-13T08:00:00.000Z", completedAt: "2026-11-13T08:30:00.000Z"),
        Job(id: "f", runnerDeviceId: "d1", check: "docs", runNumber: 8, repo: "niach/spezl", status: "completed", conclusion: "success", startedAt: "2026-11-13T07:00:00.000Z", completedAt: "2026-11-13T07:00:20.000Z"),
        Job(id: "g", runnerDeviceId: "d1", check: "old", runNumber: 7, repo: "niach/spezl", status: "completed", conclusion: "success", startedAt: "2026-11-13T06:00:00.000Z", completedAt: "2026-11-13T06:00:20.000Z"),
    ]
}

extension CiRun {
    // MARK: - The run page specimen (fixture `specimen`)

    public static let specimenNow = "2026-11-13T10:14:00.000Z"
    public static let specimenSelected = "j2"
    public static let specimenRun = Run(
        id: "r12", number: 12, name: "ci", state: "failure", error: nil,
        sha: "8f3c2a9d1e7b4c6a5f0e9d8c7b6a5f4e3d2c1b0a",
        startedAt: "2026-11-13T10:00:00.000Z", completedAt: "2026-11-13T10:12:40.000Z"
    )
    public static let specimenJobs: [Job] = [
        Job(id: "j1", check: "build", position: 0, status: "completed", conclusion: "success", runnerLabel: "mint", startedAt: "2026-11-13T10:00:00.000Z", completedAt: "2026-11-13T10:04:10.000Z"),
        Job(
            id: "j2", check: "test (linux)", position: 1, status: "completed", conclusion: "failure", runnerLabel: "mint",
            startedAt: "2026-11-13T10:04:15.000Z", completedAt: "2026-11-13T10:12:40.000Z",
            steps: [
                Step(name: "Checkout", status: "completed", conclusion: "success", startedAt: 1794564255000, completedAt: 1794564259000),
                Step(name: "Install", status: "completed", conclusion: "success", startedAt: 1794564259000, completedAt: 1794564301000),
                Step(name: "Unit tests", status: "completed", conclusion: "failure", startedAt: 1794564301000, completedAt: 1794564760000),
                Step(name: "Upload report", status: "completed", conclusion: "skipped", startedAt: nil, completedAt: nil),
            ]
        ),
        Job(id: "j3", check: "deploy", position: 2, status: "completed", conclusion: "skipped", reason: "needs", completedAt: "2026-11-13T10:12:40.000Z"),
    ]
    public static let specimenLog = "##[group]Checkout 8f3c2a9d1e7b\n##[endgroup]\n##[group]Install\nbun install v1.3.14\n1652 packages installed [611.00ms]\n##[endgroup]\n##[group]Unit tests\n\u{1b}[32m✓\u{1b}[0m src/lib/ci/state.test.ts (6 tests) 12ms\n\u{1b}[32m✓\u{1b}[0m src/lib/ci/logs.test.ts (3 tests) 9ms\n\u{1b}[31m×\u{1b}[0m src/lib/ci/train.test.ts > squashes each PR in order\n  AssertionError: expected 'conflict' to be 'composed'\n Test Files  1 failed | 2 passed (3)\n##[error]exited with code 1\n##[endgroup]\n"
}
