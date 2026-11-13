import Foundation

/// #EXP-1617 E4 part 4 (packages/domain-contract fixtures/repo-ci.json: its
/// `$comment` is the spec): the repo page's Runs tab and the CI section of its
/// Settings tab. Mirrors the TS reference `src/repo-ci.ts` (desktop
/// `domain::repo_ci`, Android `RepoCi`); `RepoCiTests` replays every case,
/// comparing the WHOLE result. Run status = `CiRun.runStatus`, durations +
/// glyphs = `PrChecks`'.
public enum RepoCi {
    // MARK: - Copy (fixture `copy` / `testIds` / `limits`)

    public enum Copy {
        public enum Runs {
            public static let noRef = "—"
            public static let colStatus = "Status"
            public static let colRun = "Run"
            public static let colCommit = "Commit"
            public static let colRef = "Branch"
            public static let colTrigger = "Trigger"
            public static let colDuration = "Duration"
            public static let colStarted = "Started"
        }

        public static let triggers: [String: String] = [
            "push": "Push",
            "pull_request": "Pull request",
            "tag": "Tag",
            "merge_train": "Merge train",
            "manual": "Manual",
        ]

        public enum Ci {
            public static let section = "CI"
            public static let noPipeline = "No runs yet. Add .exponential/ci.json to run CI here."
            public static let requiredLabel = "Required checks"
            public static let requiredDescription = "Checks that must pass before the merge train lands a pull request."
            public static let requiredPlaceholder = "Check name"
        }

        public enum Secrets {
            public static let title = "Secrets"
            public static let colName = "Name"
            public static let colUpdated = "Updated"
            public static let add = "Add secret"
            public static let name = "Name"
            public static let value = "Value"
            public static let save = "Save"
            public static let cancel = "Cancel"
            public static let delete = "Delete"
            public static let deleteTitle = "Delete this secret?"
            public static let deleteConfirm = "Delete"
            public static let empty = "No secrets yet."
            public static let invalid = "Use A–Z, 0–9 and _, not starting with a digit."
        }

        public struct SwitchCopy: Equatable, Sendable {
            public let label: String
            public let description: String
        }

        public static func `switch`(_ key: Switch) -> SwitchCopy {
            switch key {
            case .githubStatuses:
                SwitchCopy(label: "Report to GitHub", description: "Our checks also show on GitHub as commit statuses.")
            case .actionsRunners:
                SwitchCopy(
                    label: "GitHub Actions on team devices",
                    description: "Team devices with a CI runner also run this repository's GitHub Actions jobs."
                )
            }
        }
    }

    public enum TestIds {
        public static let runs = "repo-runs"
        public static let runRow = "repo-run"
        public static let ci = "repo-ci"
        public static let pipelines = "repo-ci-pipelines"
        public static let required = "repo-ci-required"
        public static let secrets = "repo-ci-secrets"
        public static let addSecret = "repo-ci-add-secret"
        public static let githubStatuses = "repo-ci-github-statuses"
        public static let actionsRunners = "repo-ci-actions-runners"
    }

    /// `limits.runs`.
    public static let maxRuns = 50

    /// The GitHub-only switches (raw value = the `ci_config` key).
    public enum Switch: String, CaseIterable, Sendable {
        case githubStatuses, actionsRunners

        public var testId: String {
            switch self {
            case .githubStatuses: TestIds.githubStatuses
            case .actionsRunners: TestIds.actionsRunners
            }
        }
    }

    // MARK: - Rows

    /// One `ci_runs` row as the Runs tab reads it.
    public struct Run: Equatable, Sendable {
        public let id: String
        public let number: Int
        public let name: String
        public let sha: String
        public let ref: String?
        public let trigger: String
        public let state: String
        public let startedAt: String?
        public let completedAt: String?

        public init(
            id: String, number: Int, name: String, sha: String = "", ref: String? = nil, trigger: String = "",
            state: String = "pending", startedAt: String? = nil, completedAt: String? = nil
        ) {
            self.id = id
            self.number = number
            self.name = name
            self.sha = sha
            self.ref = ref
            self.trigger = trigger
            self.state = state
            self.startedAt = startedAt
            self.completedAt = completedAt
        }
    }

    /// One `ci_jobs` row as the folds read it.
    public struct Job: Equatable, Sendable {
        public let runId: String
        public let status: String
        public let conclusion: String?
        public let check: String

        public init(runId: String, status: String = "", conclusion: String? = nil, check: String = "") {
            self.runId = runId
            self.status = status
            self.conclusion = conclusion
            self.check = check
        }
    }

    public struct RunRow: Equatable, Sendable {
        public let id: String
        public let identifier: String
        public let title: String
        public let status: CiRun.Status
        public let statusLabel: String
        public let glyph: PrChecks.Glyph
        public let tone: PrChecks.Tone
        public let commit: String
        public let ref: String
        public let trigger: String
        /// Phones: `ref · sha7 · trigger`.
        public let subtitle: String
        public let duration: String?
        public let startedAt: String?
    }

    public struct PipelineRow: Equatable, Sendable {
        public let name: String
        public let runId: String
        /// pending | success | failure (repo-page.json `copy.checks`).
        public let state: String
    }

    /// One secret's NAME (values are never read back).
    public struct Secret: Equatable, Sendable {
        public let name: String
        public let updatedAt: String

        public init(name: String, updatedAt: String) {
            self.name = name
            self.updatedAt = updatedAt
        }
    }

    // MARK: - Rules

    private static func glyph(_ status: CiRun.Status) -> (PrChecks.Glyph, PrChecks.Tone) {
        switch status {
        case .queued: (.queued, .muted)
        case .running: (.running, .muted)
        case .passed: (.passed, .green)
        case .failed: (.failed, .red)
        case .cancelled: (.skipped, .muted)
        }
    }

    public static func triggerLabel(_ trigger: String) -> String { Copy.triggers[trigger] ?? trigger }

    /// The Runs tab: newest first, at most `maxRuns`.
    public static func runRows(_ runs: [Run], jobs: [Job], now: Date) -> [RunRow] {
        let at = now.timeIntervalSince1970 * 1000
        let byRun = Dictionary(grouping: jobs, by: \.runId)
        return runs.enumerated()
            .sorted { $0.element.number != $1.element.number ? $0.element.number > $1.element.number : $0.offset < $1.offset }
            .prefix(maxRuns)
            .map { _, run in
                let (status, _) = CiRun.runStatus(state: run.state, jobs: (byRun[run.id] ?? []).map { ($0.status, $0.conclusion) })
                let (glyph, tone) = glyph(status)
                let commit = String(run.sha.prefix(7))
                let ref = run.ref ?? Copy.Runs.noRef
                let trigger = triggerLabel(run.trigger)
                return RunRow(
                    id: run.id,
                    identifier: "#\(run.number)",
                    title: run.name,
                    status: status,
                    statusLabel: CiRun.statusLabel(status),
                    glyph: glyph,
                    tone: tone,
                    commit: commit,
                    ref: ref,
                    trigger: trigger,
                    subtitle: "\(ref) · \(commit) · \(trigger)",
                    duration: CiRun.ms(run.startedAt).map { PrChecks.formatDuration(ms: (CiRun.ms(run.completedAt) ?? at) - $0) },
                    startedAt: run.startedAt
                )
            }
    }

    /// One row per pipeline NAME (by name, code-point order): its newest run's state.
    public static func pipelineRows(_ runs: [Run]) -> [PipelineRow] {
        var newest: [String: Run] = [:]
        for run in runs {
            if let seen = newest[run.name], run.number <= seen.number { continue }
            newest[run.name] = run
        }
        return newest.values
            .sorted { RepoPage.codePointLess($0.name, $1.name) }
            .map { run in
                PipelineRow(
                    name: run.name, runId: run.id,
                    state: run.state == "success" || run.state == "failure" ? run.state : "pending"
                )
            }
    }

    /// The checks of each pipeline's newest run, minus the required ones, sorted.
    public static func requiredSuggestions(_ runs: [Run], jobs: [Job], required: [String]) -> [String] {
        let newest = Set(pipelineRows(runs).map(\.runId))
        var names = Set(jobs.filter { newest.contains($0.runId) }.map(\.check))
        names.subtract(required)
        return names.sorted(by: RepoPage.codePointLess)
    }

    /// A secret name the server stores (an env var name, ≤ 128).
    public static func secretName(_ input: String) -> Bool {
        guard input.count <= 128, let first = input.unicodeScalars.first else { return false }
        let head = first == "_" || ("A" ... "Z").contains(first)
        return head && input.unicodeScalars.allSatisfy { $0 == "_" || ("A" ... "Z").contains($0) || ("0" ... "9").contains($0) }
    }

    /// The GitHub-only switches of the CI section, in order.
    public static func switches(provider: String?) -> [Switch] {
        provider == "github" ? [.githubStatuses, .actionsRunners] : []
    }

    // MARK: - The styleguide specimen (sg_repo-runs, sg_repo-ci): runRows[0]

    public static let specimenNow = "2026-11-13T10:30:00.000Z"
    public static let specimenRuns: [Run] = [
        Run(id: "r10", number: 10, name: "ci", sha: "aaaaaaa1111111", ref: "main", trigger: "push", state: "success",
            startedAt: "2026-11-13T09:00:00.000Z", completedAt: "2026-11-13T09:04:30.000Z"),
        Run(id: "r12", number: 12, name: "ci", sha: "ccccccc3333333", ref: "feat/x", trigger: "pull_request", state: "pending",
            startedAt: "2026-11-13T10:25:00.000Z"),
        Run(id: "r11", number: 11, name: "nightly", sha: "bbbbbbb2222222", ref: nil, trigger: "manual", state: "failure",
            startedAt: "2026-11-13T10:00:00.000Z", completedAt: "2026-11-13T10:01:00.000Z"),
        Run(id: "r13", number: 13, name: "ci", sha: "ddddddd4444444", ref: "main", trigger: "merge_train", state: "pending"),
        Run(id: "r9", number: 9, name: "ci", sha: "eeeeeee5555555", ref: "v1.0", trigger: "tag", state: "failure",
            startedAt: "2026-11-13T08:00:00.000Z", completedAt: "2026-11-13T08:00:10.000Z"),
    ]
    public static let specimenJobs: [Job] = [
        Job(runId: "r12", status: "in_progress", check: "test (linux)"),
        Job(runId: "r12", status: "completed", conclusion: "success", check: "build"),
        Job(runId: "r11", status: "completed", conclusion: "failure", check: "nightly"),
        Job(runId: "r13", status: "queued", check: "build"),
        Job(runId: "r9", status: "completed", conclusion: "cancelled", check: "build"),
        Job(runId: "r10", status: "completed", conclusion: "success", check: "build"),
    ]
    public static let specimenRequired = ["build"]
    public static let specimenSecrets: [Secret] = [
        Secret(name: "NPM_TOKEN", updatedAt: "2026-11-12T09:00:00.000Z"),
        Secret(name: "SENTRY_AUTH", updatedAt: "2026-11-06T09:00:00.000Z"),
    ]
}
