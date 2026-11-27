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
        public static let rerunAll = "Re-run all jobs"
        public static let fix = "Fix with an agent"
        public static let openExternal = "Open on GitHub"
        public static let breadcrumbRoot = "CI"
        public static let pullRequest = "Pull request #{number}"
        public static let sources: [String: String] = ["exponential": "Exponential", "github": "GitHub Actions"]
        public static let fixPromptHead = "CI run #{number} ({name}) failed on {repository} at {ref} ({sha})."
        public static let fixPromptJob = "Failing job: {job}."
        public static let fixPromptRead = "Read its whole log with exponential_repositories {op: \"run_log\", runId: \"{runId}\"}, find the cause, fix it on {ref} and push."
        public static let fixPromptTail = "The end of its log:"
        public static let emptyLog = "No log yet."
        public static let pipelineError = "The pipeline file did not plan"
        public static let none = "—"
        /// #EXP-2172: the ⋯ menu's label.
        public static let more = "More actions"
        public static let reasons: [String: String] = [
            "fork": "Not run: a pull request from a fork",
            "paths": "Skipped: no changed path matches",
            "needs": "Skipped: a job it needs did not pass",
            "runner_lost": "The runner stopped answering",
            "reused": "Kept from the run it re-runs",
        ]
        /// The Jobs section's caption parts (by job status).
        public static let jobsCaption: [Status: String] = [
            .failed: "{count} failed", .running: "{count} running", .queued: "{count} queued",
            .passed: "{count} passed", .cancelled: "{count} skipped",
        ]
        /// The selected job's card line.
        public static let jobSummaryFailed = "Failed after {duration}"
        public static let jobSummaryRunning = "Running for {duration}"
        public static let jobSummaryPassed = "Passed in {duration}"
        public static let jobSummaryQueued = "Waiting for a runner"
        public static let jobSummarySkipped = "Skipped"
        public static let jobSummaryOn = " on {runner}"
        /// The failure summary's title.
        public static let failureTitle = "Failed in {job}"

        public static func title(_ number: Int) -> String {
            title.replacingOccurrences(of: "{number}", with: "\(number)")
        }
    }

    public enum TestIds {
        public static let page = "ci-run"
        public static let jobRow = "ci-run-job"
        public static let steps = "ci-run-steps"
        public static let log = "ci-run-log"
        public static let rerun = "ci-run-rerun"
        public static let cancel = "ci-run-cancel"
        public static let rerunAll = "ci-run-rerun-all"
        public static let fix = "ci-run-fix"
        public static let openExternal = "ci-run-open-external"
        public static let breadcrumb = "ci-run-breadcrumb"
        public static let source = "ci-run-source"
        public static let chips = "ci-run-chips"
        public static let more = "ci-run-more"
        public static let failure = "ci-run-failure"
        public static let jobCard = "ci-run-job-card"
    }

    public static let liveTailMs = 2000
    public static let logMaxLinesWide = 32
    public static let logMaxLinesPhone = 20
    /// Fix with an agent quotes this many last log lines.
    public static let fixLogLines = 40

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
        /// #EXP-2172: the run's ref, source and GitHub handle.
        public let ref: String?
        public let provider: String
        public let externalUrl: String?
        public let externalAccess: String?
        /// #EXP-2172 redesign: what the run is about (nil = the pipeline's
        /// name) and when it was created (the age when it never started).
        public let title: String?
        public let createdAt: String?

        public init(
            id: String, number: Int, name: String, state: String, error: String?, sha: String, startedAt: String?,
            completedAt: String?, ref: String? = nil, provider: String = "exponential", externalUrl: String? = nil,
            externalAccess: String? = nil, title: String? = nil, createdAt: String? = nil
        ) {
            self.title = title
            self.createdAt = createdAt
            self.ref = ref
            self.provider = provider
            self.externalUrl = externalUrl
            self.externalAccess = externalAccess
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
    public enum TraceStatus: String, Sendable { case pending, running, done, skipped, error }

    public struct JobRow: Equatable, Sendable {
        public let id: String
        public let title: String
        public let subtitle: String?
        public let glyph: PrChecks.Glyph
        public let tone: PrChecks.Tone
        public let duration: String?
        /// #EXP-2172: the run page's status and its catalog tone
        /// (`CiPage.statusTone`); nil on the runner section's rows.
        public var status: Status? = nil
        public var statusTone: String? = nil
    }

    /// The run's pull request (number + base branch).
    public struct PullRequest: Equatable, Sendable {
        public let number: Int
        public let base: String

        public init(number: Int, base: String) {
            self.number = number
            self.base = base
        }
    }

    /// One chip of the strip under the title.
    public struct Chip: Equatable, Sendable {
        /// status | ref | sha | pr | author | duration | runner.
        public let id: String
        public let label: String
        public let icon: String?
        /// The status chip's dot (a colour token), else nil.
        public let dot: String?
    }

    public struct FailureSummary: Equatable, Sendable {
        public let title: String
        public let message: String
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
        public enum Primary: String, Sendable { case rerunFailed, rerunAll, cancel }
        public enum MenuItem: String, Sendable { case rerunAll, openExternal }

        /// THE one default button: Cancel while it runs or waits, Re-run
        /// failed after a failure, Re-run all after a pass; nil when the run
        /// is not ours to change (a GitHub run without Actions: write).
        public let primary: Primary?
        /// Fix with an agent (outline), after a failure.
        public let fix: Bool
        /// The ⋯ menu, in order; empty = no menu.
        public let menu: [MenuItem]
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

    /// The run's status and its catalog tone (ci-page.json `statusTones`,
    /// `CiPage.statusTone`: running is the only `info`).
    public static func runStatus(state: String, jobs: [(status: String, conclusion: String?)]) -> (status: Status, tone: String) {
        let status: Status
        if state == "pending" {
            status = jobs.contains { $0.status == "in_progress" } ? .running : .queued
        } else if state == "success" {
            status = .passed
        } else {
            let failing = jobs.filter { isFailing(status: $0.status, conclusion: $0.conclusion) }
            status = !failing.isEmpty && failing.allSatisfy { $0.conclusion == "cancelled" } ? .cancelled : .failed
        }
        return (status, CiPage.statusTone(status).tone)
    }

    static func ms(_ value: String?) -> Double? {
        guard let value, !value.isEmpty, let date = WireTimestamps.parse(value) else { return nil }
        return date.timeIntervalSince1970 * 1000
    }

    /// A job's status in the run page's terms.
    public static func jobStatus(status: String, conclusion: String?) -> Status {
        if status == "queued" { return .queued }
        if status != "completed" { return .running }
        if isFailing(status: status, conclusion: conclusion), conclusion != "cancelled" { return .failed }
        if conclusion == "success" { return .passed }
        return .cancelled
    }

    public static func jobStatus(_ job: Job) -> Status { jobStatus(status: job.status, conclusion: job.conclusion) }

    /// The Jobs section's caption: `1 failed · 3 passed · 1 skipped` (the
    /// counts that are not zero, in this order).
    public static func jobsCaption(_ jobs: [Job]) -> String {
        var counts: [Status: Int] = [:]
        for job in jobs { counts[jobStatus(job), default: 0] += 1 }
        let order: [Status] = [.failed, .running, .queued, .passed, .cancelled]
        return order.compactMap { status -> String? in
            guard let count = counts[status], count > 0 else { return nil }
            return Copy.jobsCaption[status]?.replacingOccurrences(of: "{count}", with: String(count))
        }.joined(separator: " · ")
    }

    /// The selected job's card line: `Failed after 8m 25s on mint`.
    public static func jobSummary(_ job: Job, now: Date) -> String {
        let status = jobStatus(job)
        let duration = PrChecks.checkDuration(job.asCheck, now: now)
        let template: String
        switch status {
        case .queued: return Copy.jobSummaryQueued
        case .cancelled: return reasonText(job.reason) ?? Copy.jobSummarySkipped
        case .failed: template = Copy.jobSummaryFailed
        case .running: template = Copy.jobSummaryRunning
        case .passed: template = Copy.jobSummaryPassed
        }
        let head = template.replacingOccurrences(of: "{duration}", with: duration ?? Copy.none)
        guard let runner = job.runnerLabel, !runner.isEmpty else { return head }
        return head + Copy.jobSummaryOn.replacingOccurrences(of: "{runner}", with: runner)
    }

    /// The colour token of a status chip's dot, by tone.
    private static func dot(_ tone: String) -> String {
        switch tone {
        case "success": "$color.success"
        case "danger": "$color.destructive"
        case "info": "$color.info"
        default: "$color.mutedForeground"
        }
    }

    /// The chip strip under the title: status (with its dot) · ref → base ·
    /// sha · #PR · author · duration · runners; a chip without data is left out.
    public static func chips(run: Run, jobs: [Job], pullRequest: PullRequest?, author: String?, now: Date) -> [Chip] {
        let (status, tone) = runStatus(state: run.state, jobs: jobs.map { ($0.status, $0.conclusion) })
        let started = ms(run.startedAt)
        let end = ms(run.completedAt) ?? now.timeIntervalSince1970 * 1000
        var runners: [String] = []
        for job in jobs {
            if let label = job.runnerLabel, !label.isEmpty, !runners.contains(label) { runners.append(label) }
        }
        var chips = [Chip(id: "status", label: statusLabel(status), icon: nil, dot: dot(tone))]
        if let ref = run.ref, !ref.isEmpty {
            chips.append(Chip(id: "ref", label: pullRequest.map { "\(ref) → \($0.base)" } ?? ref, icon: "git-branch", dot: nil))
        }
        chips.append(Chip(id: "sha", label: String(run.sha.prefix(7)), icon: "hash", dot: nil))
        if let pullRequest { chips.append(Chip(id: "pr", label: "#\(pullRequest.number)", icon: "git-pull-request", dot: nil)) }
        if let author, !author.isEmpty { chips.append(Chip(id: "author", label: author, icon: "user", dot: nil)) }
        if let started { chips.append(Chip(id: "duration", label: PrChecks.formatDuration(ms: end - started), icon: "clock", dot: nil)) }
        if !runners.isEmpty { chips.append(Chip(id: "runner", label: runners.joined(separator: ", "), icon: "server", dot: nil)) }
        return chips
    }

    /// The failure summary above the jobs, from the failed job's log as
    /// read: `Failed in {job}` and its first error line. Nil until such a
    /// line is read (logs are read on demand; no test counting).
    public static func failureSummary(check: String?, log: String?) -> FailureSummary? {
        guard let check, let log, !log.isEmpty else { return nil }
        let view = logView(log)
        guard let first = view.highlight.first else { return nil }
        let lines = view.code.components(separatedBy: "\n")
        var line = first - 1 < lines.count ? lines[first - 1] : ""
        if line.hasPrefix("Error: ") { line.removeFirst("Error: ".count) }
        return FailureSummary(title: Copy.failureTitle.replacingOccurrences(of: "{job}", with: check), message: line)
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
                duration: PrChecks.checkDuration(job.asCheck, now: now),
                status: jobStatus(job),
                statusTone: CiPage.statusTone(jobStatus(job)).tone
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

    /// Which actions the run offers (#EXP-2172, the approved redesign; a
    /// GitHub run re-runs and cancels only once its installation accepted
    /// Actions: write).
    public static func actions(
        state: String, error: String?, provider: String? = nil, externalAccess: String? = nil, externalUrl: String? = nil
    ) -> Actions {
        let github = provider == "github"
        let writable = !github || externalAccess == "write"
        let primary: Actions.Primary? = !writable ? nil : state == "pending" ? .cancel : state == "failure" ? .rerunFailed : .rerunAll
        var menu: [Actions.MenuItem] = []
        if writable, primary != .rerunAll { menu.append(.rerunAll) }
        if github, !(externalUrl ?? "").isEmpty { menu.append(.openExternal) }
        return Actions(primary: primary, fix: state == "failure", menu: menu)
    }

    public static func actions(_ run: Run) -> Actions {
        actions(state: run.state, error: run.error, provider: run.provider, externalAccess: run.externalAccess, externalUrl: run.externalUrl)
    }

    /// The run's source: `github` for a GitHub run, else ours.
    public static func source(_ provider: String?) -> String {
        provider == "github" ? "github" : "exponential"
    }

    /// The first message of Fix with an agent (the composer's text).
    public static func fixPrompt(run: Run, repository: String, job: String?, log: String?) -> String {
        let ref = run.ref ?? Copy.none
        func fill(_ text: String) -> String {
            text.replacingOccurrences(of: "{number}", with: String(run.number))
                .replacingOccurrences(of: "{name}", with: run.name)
                .replacingOccurrences(of: "{repository}", with: repository)
                .replacingOccurrences(of: "{ref}", with: ref)
                .replacingOccurrences(of: "{sha}", with: String(run.sha.prefix(7)))
                .replacingOccurrences(of: "{runId}", with: run.id)
                .replacingOccurrences(of: "{job}", with: job ?? "")
        }
        var lines = [fill(Copy.fixPromptHead)]
        if let job, !job.isEmpty { lines.append(fill(Copy.fixPromptJob)) }
        lines.append(fill(Copy.fixPromptRead))
        let code = log.map { logView($0).code } ?? ""
        if !code.isEmpty {
            let tail = code.components(separatedBy: "\n").suffix(fixLogLines)
            lines.append(contentsOf: ["", Copy.fixPromptTail, "```text"] + tail + ["```"])
        }
        return lines.joined(separator: "\n")
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
        public static let memory = "Memory per job"
        public static let pause = "Pause for games and full-screen apps"
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
        "unsupported": Notice(type: "info", title: "Not on this computer yet", message: "A job could read this app's data on macOS and Windows. Run CI on Linux or a server."),
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
        public static let memory = "runner-memory"
        public static let pause = "runner-pause"
    }

    public static let maxLabels = 16
    public static let minSlots = 1
    public static let maxSlots = 16
    public static let maxRecent = 5
    public static let minMemoryMb = 1024
    public static let maxMemoryMb = 262_144
    public static let memoryStepMb = 1024
    public static let timeoutOptions: [(value: String, label: String)] = [
        ("15", "15 min"), ("30", "30 min"), ("60", "1 h"), ("120", "2 h"), ("240", "4 h"), ("360", "6 h"),
    ]
    public static let memoryOptions: [(value: String, label: String)] = [
        ("2048", "2 GB"), ("4096", "4 GB"), ("8192", "8 GB"), ("16384", "16 GB"), ("32768", "32 GB"), ("65536", "64 GB"),
    ]

    public struct Settings: Equatable, Sendable {
        public var enabled: Bool
        public var labels: [String]
        public var slots: Int
        public var timeoutMin: Int
        /// #EXP-2193: the job's memory cap; pause jobs while a game runs.
        public var memoryMaxMb: Int
        public var pauseWhenBusy: Bool

        public init(
            enabled: Bool, labels: [String], slots: Int, timeoutMin: Int,
            memoryMaxMb: Int = 8192, pauseWhenBusy: Bool = true
        ) {
            self.enabled = enabled
            self.labels = labels
            self.slots = slots
            self.timeoutMin = timeoutMin
            self.memoryMaxMb = memoryMaxMb
            self.pauseWhenBusy = pauseWhenBusy
        }

        public static let defaults = Settings(
            enabled: false, labels: [], slots: 1, timeoutMin: 60, memoryMaxMb: 8192, pauseWhenBusy: true
        )

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
            memoryMaxMb = (object?["memoryMaxMb"] as? NSNumber)?.intValue ?? d.memoryMaxMb
            pauseWhenBusy = (object?["pauseWhenBusy"] as? Bool) ?? d.pauseWhenBusy
        }
    }

    /// A typed label as stored, nil = refused.
    public static func normalizeLabel(_ input: String) -> String? {
        let label = input.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard label.range(of: "^[a-z0-9][a-z0-9._-]{0,31}$", options: .regularExpression) != nil else { return nil }
        return label
    }

    private static let phones: Set<String> = ["ios", "android", "web-mobile"]

    /// #EXP-2193: the desktop app on macOS or Windows gets no runner yet (a
    /// job could read the app's data there); a server daemon on any OS keeps it.
    public static func unsupported(kind: String?, platform: String?) -> Bool {
        kind == "desktop" && (platform == "macos" || platform == "darwin" || platform == "windows")
    }

    /// The section's notice key: an unsupported desktop, phones never run
    /// jobs, an old app cannot.
    public static func notice(client: String, caps: [String], kind: String?, platform: String?) -> String? {
        if unsupported(kind: kind, platform: platform) { return "unsupported" }
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
    public static let specimenKind = "desktop"
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
        startedAt: "2026-11-13T10:00:00.000Z", completedAt: "2026-11-13T10:12:40.000Z",
        ref: "exp/APP-33", provider: "exponential",
        title: "Mirror prunes deleted branches", createdAt: "2026-11-13T09:59:50.000Z"
    )
    public static let specimenRepository = "acme/engine"
    public static let specimenPullRequest = PullRequest(number: 213, base: "main")
    public static let specimenAuthor = "Mara Lind"
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
