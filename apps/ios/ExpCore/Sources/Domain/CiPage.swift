import Foundation

/// #EXP-2172 (packages/domain-contract fixtures/ci-page.json is the spec; TS
/// reference `src/ci-page.ts`, desktop `domain::ci_page`, Android `CiPage`):
/// THE CI page (one root destination across every member team) and its
/// badge. #EXP-2214 (Danny 2026-11-28, "the ci page makes no sense"): four
/// tabs, one meaning each: the MERGE QUEUE (what is being tested, what is
/// next, what was sent back, what landed; `CiPageQueue.swift`), PULL REQUESTS
/// (one row per PR, its runs folded under it), RELEASES (tags, manual,
/// scheduled and default-branch runs) and RUNNERS. Pure, replayed by
/// `CiPageTests`.
public enum CiPage {
    // MARK: - Copy, limits, test ids (fixture `copy` / `limits` / `testIds`)

    public enum Copy {
        public static let title = "CI"
        public static let tabs: [String: String] = [
            "train": "Merge queue", "prs": "Pull requests", "releases": "Releases", "runners": "Runners",
        ]
        /// Phones' tab labels.
        public static let phoneTabs: [String: String] = [
            "train": "Queue", "prs": "PRs", "releases": "Releases", "runners": "Runners",
        ]
        public static let filters: [String: String] = ["all": "All runs", "running": "Running", "failed": "Failed", "mine": "Mine"]
        public static let filterLabels: [String: String] = ["status": "Status", "repository": "Repository", "source": "Source"]
        public static let allRepositories = "All repositories"
        public static let sources: [String: String] = ["all": "Every source", "exponential": "Exponential", "github": "GitHub Actions"]
        public static let columns: [String: String] = [
            "status": "Status", "run": "Run", "repository": "Repository", "source": "Source",
            "ref": "Branch", "trigger": "Trigger", "duration": "Duration", "started": "Started",
        ]
        public static let pullRequest = "Pull request #{number}"
        public static let triggers: [String: String] = [
            "push": "Push", "pull_request": "Pull request", "tag": "Tag",
            "merge_train": "Merge queue", "manual": "Manual", "schedule": "Schedule",
        ]
        public static let unknownRepository = "Repository"
        public static let inProgress = "In progress"
        public static let today = "Today"
        public static let yesterday = "Yesterday"
        public static let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        public static let queued = "queued"
        public static let batchTitle = "Testing {numbers} together"
        /// A merge queue run of one PR.
        public static let batchOne = "Testing {number}"
        public static let runnerChip = "{online} of {total} runners"
        public static let waitingForRunner = "waiting for a runner"
        /// What a run is for (`runKind`): the run page's kind Badge.
        public static let kinds: [String: String] = [
            "train": "Merge queue", "pull_request": "Pull request", "release": "Release", "branch": "Branch",
        ]
        public enum Prs {
            public static let open = "Open"
            public static let branches = "Branches"
            public static let closed = "Merged or closed"
        }
        public enum Queue {
            public static let testing = "Testing now"
            public static let base = "{base} @ {sha}"
            public static let rerun = "re-run"
            public static let split = "split: {count} more to test"
            public static let untested = "Merge now, untested"
            public static let runningFor = "Running for {duration}"
            public static let left = "about {n} min left"
            public static let almostDone = "almost done"
            public static let batch = "In this batch"
            public static let openRun = "Open run"
            public static let upNext = "Up next"
            public static let position = "{ordinal} in line"
            public static let mergeNow = "Merge now · skips CI, lands next"
            public static let notReady = "Not ready yet"
            public static let sentBack = "Sent back"
            public static let failedIn = "Failed in {checks}"
            public static let conflictIn = "Conflict in {files}"
            public static let tries = "{n} of {max} tries"
            public static let landed = "Landed"
            public static let landedWith = "with {numbers}"
            public static let mergedNow = "Merge now · untested"
            public static let openLog = "Open log"
            public static let resume = "Resume"
            public static let more = "More"
            public static let menu: [String: String] = ["mergeNow": "Merge now", "hold": "Hold"]
            public static let into = "into {base}"
            public static let stats: [String: String] = [
                "inQueue": "In the queue", "landedToday": "Landed today",
                "sentBackToday": "Sent back today", "typicalRun": "Typical run",
            ]
            public static let waits: [String: String] = [
                "checks": "Checks running", "session": "Session working", "approval": "Needs approval",
                "stack": "Waiting for its base", "draft": "Draft", "failing": "Checks failing", "conflict": "Conflict",
            ]
            public static let holds: [String: String] = [
                "person": "On hold", "bounces": "On hold after 3 tries", "timeout": "On hold after 24 hours",
                "no_checks": "On hold: no checks", "conflict": "On hold: conflict",
            ]
            public static let whatHappened = "What happened"
            public static let outcome: [String: String] = [
                "landed": "Landed as {sha}", "untested": "Merge now, landed untested",
                "failing": "Sent back: failed in {checks} · {tries}", "conflict": "Sent back: conflict in {files} · {tries}",
                "testing": "Being tested", "requeued": "Back in the queue",
            ]
        }
        public enum Runners {
            public static let busy = "{busy} of {slots} busy"
            public static let queue = "{count} queued"
            public static let offlineSince = "Last seen {time}"
            public static let noLabels = "No labels"
        }
        public enum Empty {
            public static let runs = "No runs yet"
            public static let filtered = "No runs match"
            public static let train = "Nothing in the merge queue"
            public static let prs = "No pull request runs yet"
            public static let releases = "No release runs yet"
            public static let runners = "No runners"
        }
    }

    public enum Limits {
        public static let runs = 100
        /// A runner is online while its last beat is younger than this.
        public static let runnerOnlineMs: Double = 120_000
        /// Per Pull requests section.
        public static let subjects = 50
        /// The runs folded under one PR.
        public static let subjectRuns = 10
        /// Sent back / Landed rows per repository.
        public static let history = 10
        public static let historyHours: Double = 48
        /// Finished green merge queue runs before a typical run exists.
        public static let etaSamples = 3
    }

    public enum TestIds {
        public static let page = "ci-page"
        public static let tabs = "ci-tabs"
        public static let filters = "ci-filters"
        public static let statusFilter = "ci-filter-status"
        public static let repoFilter = "ci-filter-repo"
        public static let sourceFilter = "ci-filter-source"
        public static let runs = "ci-runs"
        public static let runRow = "ci-run-row"
        public static let runners = "ci-runners"
        public static let runnerRow = "ci-runner-row"
        public static let runnerChip = "ci-runner-chip"
        public static let queue = "ci-queue"
        public static let queueRepo = "ci-queue-repo"
        public static let queueStats = "ci-queue-stats"
        public static let queueNow = "ci-queue-now"
        public static let queueRow = "ci-queue-row"
        public static let prs = "ci-prs"
        public static let prRow = "ci-pr-row"
        public static let releases = "ci-releases"
    }

    // MARK: - Inputs

    public enum Source: String, Sendable, Equatable { case exponential, github }
    public enum Filter: String, Sendable, Equatable, CaseIterable { case all, running, failed, mine }
    /// The tab order (the merge queue opens first).
    public enum Tab: String, Sendable, Equatable, CaseIterable { case train, prs, releases, runners }
    /// #EXP-2214: what a run is for (`runKind`).
    public enum RunKind: String, Sendable, Equatable { case train, pullRequest = "pull_request", release, branch }

    public struct Run: Equatable, Sendable {
        public let id: String
        public let repositoryId: String
        public let number: Int
        public let name: String
        public let sha: String
        public let ref: String?
        public let trigger: String
        public let state: String
        public let pullRequestId: String?
        public let createdByUserId: String?
        public let provider: String
        public let startedAt: String?
        public let completedAt: String?
        public let createdAt: String
        /// What the run is about (`ci_runs.title`).
        public let title: String?
        /// #EXP-2214: the merge queue run it tests (`ci_runs.merge_train_id`).
        public let mergeTrainId: String?

        public init(
            id: String, repositoryId: String, number: Int, name: String, sha: String = "", ref: String?,
            trigger: String, state: String, pullRequestId: String?, createdByUserId: String?, provider: String,
            startedAt: String?, completedAt: String?, createdAt: String, title: String? = nil, mergeTrainId: String? = nil
        ) {
            self.title = title
            self.id = id
            self.repositoryId = repositoryId
            self.number = number
            self.name = name
            self.sha = sha
            self.ref = ref
            self.trigger = trigger
            self.state = state
            self.pullRequestId = pullRequestId
            self.createdByUserId = createdByUserId
            self.provider = provider
            self.startedAt = startedAt
            self.completedAt = completedAt
            self.createdAt = createdAt
            self.mergeTrainId = mergeTrainId
        }
    }

    public struct Job: Equatable, Sendable {
        /// #EXP-2214: the Sent back row's Open log names a job.
        public let id: String
        public let runId: String
        public let check: String
        public let status: String
        public let conclusion: String?
        public let runsOn: [String]
        public let runnerDeviceId: String?
        public let provider: String

        public init(
            id: String = "", runId: String, check: String = "", status: String, conclusion: String?, runsOn: [String],
            runnerDeviceId: String?, provider: String
        ) {
            self.id = id
            self.runId = runId
            self.check = check
            self.status = status
            self.conclusion = conclusion
            self.runsOn = runsOn
            self.runnerDeviceId = runnerDeviceId
            self.provider = provider
        }
    }

    public struct Repo: Equatable, Sendable {
        public let id: String
        public let fullName: String
        /// #EXP-2214: a push to it is a release run (nil = `main`).
        public let defaultBranch: String?

        public init(id: String, fullName: String, defaultBranch: String? = nil) {
            self.id = id
            self.fullName = fullName
            self.defaultBranch = defaultBranch
        }
    }

    public struct Pr: Equatable, Sendable {
        public let id: String
        public let number: Int
        public let title: String
        public let repositoryId: String
        public let rootSessionId: String?
        public let base: String
        /// #EXP-2214: open | draft | merged | closed.
        public let state: String
        public let mergeState: String?
        /// What a `checks` PR waits for (merge-train.json `mergeWait`).
        public let mergeWait: String?
        /// person | bounces | timeout | no_checks | conflict.
        public let mergeHold: String?
        public let mergeNow: Bool
        public let mergeAttempts: Int
        public let mergePosition: Int?
        public let mergeTrainId: String?

        public init(
            id: String, number: Int, title: String, repositoryId: String, rootSessionId: String?, base: String,
            state: String = "open", mergeState: String?, mergeWait: String? = nil, mergeHold: String? = nil,
            mergeNow: Bool = false, mergeAttempts: Int = 0, mergePosition: Int?, mergeTrainId: String?
        ) {
            self.id = id
            self.number = number
            self.title = title
            self.repositoryId = repositoryId
            self.rootSessionId = rootSessionId
            self.base = base
            self.state = state
            self.mergeState = mergeState
            self.mergeWait = mergeWait
            self.mergeHold = mergeHold
            self.mergeNow = mergeNow
            self.mergeAttempts = mergeAttempts
            self.mergePosition = mergePosition
            self.mergeTrainId = mergeTrainId
        }
    }

    /// #EXP-2214: one merge queue run (`merge_trains`, the synced columns).
    public struct Train: Equatable, Sendable {
        public let id: String
        public let repositoryId: String
        public let base: String
        public let baseSha: String?
        /// PR ids per unit (a stack bottom first).
        public let units: [[String]]
        public let state: String
        public let attempt: Int
        public let bisectOf: String?
        /// The bisect halves still to test after it.
        public let pending: [[[String]]]
        public let untested: Bool
        public let testedSha: String?
        public let createdAt: String
        public let finishedAt: String?

        public init(
            id: String, repositoryId: String, base: String, baseSha: String?, units: [[String]], state: String,
            attempt: Int = 0, bisectOf: String? = nil, pending: [[[String]]] = [], untested: Bool = false,
            testedSha: String? = nil, createdAt: String, finishedAt: String? = nil
        ) {
            self.id = id
            self.repositoryId = repositoryId
            self.base = base
            self.baseSha = baseSha
            self.units = units
            self.state = state
            self.attempt = attempt
            self.bisectOf = bisectOf
            self.pending = pending
            self.untested = untested
            self.testedSha = testedSha
            self.createdAt = createdAt
            self.finishedAt = finishedAt
        }
    }

    /// #EXP-2214: one row of the queue's history (`merge_train_events`).
    public struct TrainEvent: Equatable, Sendable {
        public let id: String
        public let repositoryId: String
        public let pullRequestId: String
        public let mergeTrainId: String?
        /// landed | failing | conflict | held | merge_now | resumed.
        public let kind: String
        public let base: String
        public let attempt: Int
        public let reason: String?
        public let checks: [String]
        public let ciJobId: String?
        public let jobUrl: String?
        public let files: [String]
        public let sha: String?
        public let byUserId: String?
        public let createdAt: String

        public init(
            id: String, repositoryId: String, pullRequestId: String, mergeTrainId: String?, kind: String, base: String,
            attempt: Int = 0, reason: String? = nil, checks: [String] = [], ciJobId: String? = nil, jobUrl: String? = nil,
            files: [String] = [], sha: String? = nil, byUserId: String? = nil, createdAt: String
        ) {
            self.id = id
            self.repositoryId = repositoryId
            self.pullRequestId = pullRequestId
            self.mergeTrainId = mergeTrainId
            self.kind = kind
            self.base = base
            self.attempt = attempt
            self.reason = reason
            self.checks = checks
            self.ciJobId = ciJobId
            self.jobUrl = jobUrl
            self.files = files
            self.sha = sha
            self.byUserId = byUserId
            self.createdAt = createdAt
        }
    }

    /// A session row: its starter (`userId`).
    public struct Session: Equatable, Sendable {
        public let id: String
        public let userId: String

        public init(id: String, userId: String) {
            self.id = id
            self.userId = userId
        }
    }

    public struct Runner: Equatable, Sendable {
        public let id: String
        public let label: String
        public let platform: String?
        public let labels: [String]
        public let slots: Int
        public let lastSeenAt: String?

        public init(id: String, label: String, platform: String?, labels: [String], slots: Int, lastSeenAt: String?) {
            self.id = id
            self.label = label
            self.platform = platform
            self.labels = labels
            self.slots = slots
            self.lastSeenAt = lastSeenAt
        }
    }

    /// A person a run names (its author), from the users shape.
    public struct User: Equatable, Sendable {
        public let id: String
        public let name: String
        public let image: String?

        public init(id: String, name: String, image: String?) {
            self.id = id
            self.name = name
            self.image = image
        }
    }

    public struct Input: Equatable, Sendable {
        public var runs: [Run]
        public var jobs: [Job]
        public var repos: [Repo]
        public var pullRequests: [Pr]
        public var sessions: [Session]
        public var runners: [Runner]
        public var users: [User]
        /// #EXP-2214: the merge queue's runs and history.
        public var trains: [Train]
        public var trainEvents: [TrainEvent]
        public var viewerId: String
        public var now: String
        /// The viewer's clock offset from UTC in minutes (Today / Yesterday).
        public var utcOffsetMinutes: Int

        public init(
            runs: [Run], jobs: [Job], repos: [Repo], pullRequests: [Pr], sessions: [Session], runners: [Runner],
            users: [User] = [], trains: [Train] = [], trainEvents: [TrainEvent] = [], viewerId: String, now: String,
            utcOffsetMinutes: Int = 0
        ) {
            self.runs = runs
            self.jobs = jobs
            self.repos = repos
            self.pullRequests = pullRequests
            self.sessions = sessions
            self.runners = runners
            self.users = users
            self.trains = trains
            self.trainEvents = trainEvents
            self.viewerId = viewerId
            self.now = now
            self.utcOffsetMinutes = utcOffsetMinutes
        }
    }

    public struct RunFilter: Equatable, Sendable {
        public var status: Filter
        public var repositoryId: String?
        public var source: Source?

        public init(status: Filter = .all, repositoryId: String? = nil, source: Source? = nil) {
            self.status = status
            self.repositoryId = repositoryId
            self.source = source
        }

        public static let all = RunFilter()
    }

    // MARK: - Rows

    public struct RunListRow: Equatable, Sendable {
        public let id: String
        public let status: CiRun.Status
        public let statusLabel: String
        public let glyph: PrChecks.Glyph
        /// The catalog tone (`statusTones`).
        public let tone: String
        /// A tag run's tag, else the commit / PR line (`title`), else the
        /// merge queue batch, else the pipeline's name.
        public let title: String
        /// `name #n`.
        public let workflow: String
        public let repositoryId: String
        public let repository: String
        /// The repository's own name (after the owner).
        public let repo: String
        public let ref: String
        public let sha: String
        public let trigger: String
        public let source: Source
        public let sourceLabel: String
        /// `github` | `brand-exponential`.
        public let sourceIcon: String
        /// nil before it started.
        public let duration: String?
        /// When it started, else when it was queued (the age, relative).
        public let at: String
        public let author: User?
        /// Wide: `workflow · repo · ref · sha · trigger`.
        public let subtitle: String
        /// Phones: `repo · ref · workflow`.
        public let phoneSubtitle: String
        /// Phones' meta: the duration, else copy.queued.
        public let phoneMeta: String
    }

    public struct RunGroup: Equatable, Sendable {
        /// `in-progress`, else the local day `yyyy-mm-dd`.
        public let id: String
        public let title: String
        /// In progress only.
        public let count: Int?
        public var rows: [RunListRow]
    }

    public struct RunnerChip: Equatable, Sendable {
        public let label: String
        public let online: Bool
    }

    public struct Option: Equatable, Sendable {
        public let label: String
        public let value: String
    }

    public struct RunnerRow: Equatable, Sendable {
        public let id: String
        public let title: String
        /// The labels it takes jobs for, OS first.
        public let subtitle: String
        public let online: Bool
        /// `{busy} of {slots} busy` online; nil offline.
        public let busy: String?
        /// Our queued jobs this runner could take; nil when none.
        public let queue: String?
        /// The last beat of an offline runner (ISO); nil online or never seen.
        public let offlineSince: String?
    }

    // MARK: - Rules

    /// JS `<` on strings: UTF-16 code units, so every platform orders alike.
    static func jsLess(_ a: String, _ b: String) -> Bool {
        a.utf16.lexicographicallyPrecedes(b.utf16)
    }

    /// JS's stable `sort` with a three-way comparator (< 0 = a first).
    static func stableSorted<T>(_ items: [T], _ compare: (T, T) -> Int) -> [T] {
        items.enumerated().sorted { a, b in
            let c = compare(a.element, b.element)
            return c != 0 ? c < 0 : a.offset < b.offset
        }.map(\.element)
    }

    static func msOf(_ value: String?) -> Double? { CiRun.ms(value) }

    /// JS truthiness of an optional string (nil and `""` are falsy).
    static func present(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return value
    }

    /// The one status table (fixture `statusTones`, also the run page's):
    /// running is the only blue, moving mark; queued and cancelled stay
    /// neutral. Tone = the catalog's Tone (`neutral` | `info` | `success` | `danger`).
    public static func statusTone(_ status: CiRun.Status) -> (glyph: PrChecks.Glyph, tone: String) {
        switch status {
        case .passed: (.passed, "success")
        case .failed: (.failed, "danger")
        case .running: (.running, "info")
        case .queued: (.queued, "neutral")
        case .cancelled: (.skipped, "neutral")
        }
    }

    /// A run's source; anything but `github` is ours.
    public static func source(_ provider: String) -> Source {
        provider == "github" ? .github : .exponential
    }

    private static func runAt(_ run: Run) -> Double {
        msOf(run.startedAt) ?? msOf(run.createdAt) ?? 0
    }

    /// Newest first; ties by id.
    static func newestFirst(_ runs: [Run]) -> [Run] {
        runs.sorted { a, b in
            let (x, y) = (runAt(a), runAt(b))
            return x != y ? x > y : jsLess(a.id, b.id)
        }
    }

    static func statusOf(_ run: Run, _ jobs: [Job]) -> CiRun.Status {
        CiRun.runStatus(state: run.state, jobs: jobs.filter { $0.runId == run.id }.map { ($0.status, $0.conclusion) }).status
    }

    /// `mine`: the viewer started the run, or its pull request's chat is the
    /// viewer's.
    public static func isMine(_ run: Run, viewerId: String, pullRequests: [Pr], sessions: [Session]) -> Bool {
        if run.createdByUserId == viewerId { return true }
        guard let prId = run.pullRequestId,
              let root = pullRequests.first(where: { $0.id == prId })?.rootSessionId
        else { return false }
        return sessions.contains { $0.id == root && $0.userId == viewerId }
    }

    /// #EXP-2214 (fixture `kindCases`): what a run is for. A merge queue run
    /// (its trigger, its train, an `exp-train/` ref); a pull request's (its
    /// PR, or the pull_request trigger); a RELEASE run (a tag, a manual or
    /// scheduled run, a push to the repository's default branch, `main` when
    /// unknown); anything else is a BRANCH run (a push to another branch).
    public static func runKind(_ run: Run, repos: [Repo]) -> RunKind {
        if run.trigger == "merge_train" || present(run.mergeTrainId) != nil || (run.ref ?? "").hasPrefix("exp-train/") { return .train }
        if present(run.pullRequestId) != nil || run.trigger == "pull_request" { return .pullRequest }
        if run.trigger == "tag" || run.trigger == "manual" || run.trigger == "schedule" { return .release }
        let branch = repos.first { $0.id == run.repositoryId }?.defaultBranch ?? "main"
        return run.trigger == "push" && run.ref == branch ? .release : .branch
    }

    private static func triggerText(_ run: Run, _ pullRequests: [Pr]) -> String {
        if run.trigger == "pull_request", let prId = run.pullRequestId, let pr = pullRequests.first(where: { $0.id == prId }) {
            return Copy.pullRequest.replacingOccurrences(of: "{number}", with: String(pr.number))
        }
        return Copy.triggers[run.trigger] ?? run.trigger
    }

    public static func repoName(_ repos: [Repo], _ id: String) -> String {
        repos.first { $0.id == id }?.fullName ?? Copy.unknownRepository
    }

    static func numbers(_ list: [Int]) -> String {
        list.map { "#\($0)" }.joined(separator: ", ")
    }

    /// A merge queue run is its batch: `Testing #a, #b together` (#EXP-2214:
    /// its train's units, else, running, the repo's testing PRs).
    static func batchTitle(_ run: Run, pullRequests: [Pr], trains: [Train]) -> String? {
        if let trainId = present(run.mergeTrainId), let train = trains.first(where: { $0.id == trainId }) {
            let list = train.units.flatMap { $0 }.compactMap { id in pullRequests.first { $0.id == id }?.number }
            if list.count == 1 { return Copy.batchOne.replacingOccurrences(of: "{number}", with: "#\(list[0])") }
            if list.count > 1 { return Copy.batchTitle.replacingOccurrences(of: "{numbers}", with: numbers(list)) }
        }
        guard run.trigger == "merge_train", run.state == "pending" else { return nil }
        let members = pullRequests.filter { $0.repositoryId == run.repositoryId && $0.mergeState == "testing" }.map(\.number).sorted()
        guard !members.isEmpty else { return nil }
        return Copy.batchTitle.replacingOccurrences(of: "{numbers}", with: numbers(members))
    }

    /// One row of the list (both form factors' strings).
    public static func runRow(_ run: Run, _ input: Input) -> RunListRow {
        let at = msOf(input.now) ?? 0
        let status = statusOf(run, input.jobs)
        let source = source(run.provider)
        let repository = repoName(input.repos, run.repositoryId)
        let repo = repository.split(separator: "/", omittingEmptySubsequences: false).last.map(String.init) ?? repository
        let ref = run.ref ?? RepoCi.Copy.Runs.noRef
        let sha = String(run.sha.prefix(7))
        let workflow = "\(run.name) #\(run.number)"
        let trigger = triggerText(run, input.pullRequests)
        let pr = run.pullRequestId.flatMap { id in input.pullRequests.first { $0.id == id } }
        let duration = msOf(run.startedAt).map { PrChecks.formatDuration(ms: (msOf(run.completedAt) ?? at) - $0) }
        let (glyph, tone) = statusTone(status)
        // #EXP-2214: a tag run reads as its tag (`ios-v0.14.150`).
        let tag = run.trigger == "tag" ? present(run.ref) : nil
        return RunListRow(
            id: run.id,
            status: status,
            statusLabel: CiRun.statusLabel(status),
            glyph: glyph,
            tone: tone,
            title: tag ?? run.title ?? batchTitle(run, pullRequests: input.pullRequests, trains: input.trains) ?? pr?.title ?? run.name,
            workflow: workflow,
            repositoryId: run.repositoryId,
            repository: repository,
            repo: repo,
            ref: ref,
            sha: sha,
            trigger: trigger,
            source: source,
            sourceLabel: Copy.sources[source.rawValue] ?? source.rawValue,
            sourceIcon: source == .github ? "github" : "brand-exponential",
            duration: duration,
            at: run.startedAt ?? run.createdAt,
            author: run.createdByUserId.flatMap { id in input.users.first { $0.id == id } },
            subtitle: [workflow, repo, ref, sha, trigger].joined(separator: " · "),
            phoneSubtitle: [repo, ref, workflow].joined(separator: " · "),
            phoneMeta: duration ?? Copy.queued
        )
    }

    /// The runs of the filter (#EXP-2214: of `kinds` when given), newest
    /// first, at most `Limits.runs`.
    public static func runList(_ input: Input, filter: RunFilter, kinds: [RunKind]? = nil) -> [RunListRow] {
        var rows: [RunListRow] = []
        for run in newestFirst(input.runs) {
            if let kinds, !kinds.contains(runKind(run, repos: input.repos)) { continue }
            let status = statusOf(run, input.jobs)
            let source = source(run.provider)
            if let repo = filter.repositoryId, !repo.isEmpty, run.repositoryId != repo { continue }
            if let wanted = filter.source, source != wanted { continue }
            if filter.status == .running, status != .running, status != .queued { continue }
            if filter.status == .failed, status != .failed { continue }
            if filter.status == .mine, !isMine(run, viewerId: input.viewerId, pullRequests: input.pullRequests, sessions: input.sessions) { continue }
            rows.append(runRow(run, input))
            if rows.count >= Limits.runs { break }
        }
        return rows
    }

    private static let utc: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()

    /// The local day of an instant, `yyyy-mm-dd`, at the viewer's offset.
    static func localDay(_ ms: Double, _ offsetMinutes: Int) -> String {
        let date = Date(timeIntervalSince1970: (ms + Double(offsetMinutes) * 60_000) / 1000)
        let c = utc.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }

    /// A day's section title: Today, Yesterday, else `27 Nov` (+ the year
    /// when it is not this year's).
    public static func dayTitle(_ day: String, now: String, utcOffsetMinutes: Int) -> String {
        let nowMs = msOf(now) ?? 0
        let today = localDay(nowMs, utcOffsetMinutes)
        if day == today { return Copy.today }
        if day == localDay(nowMs - 86_400_000, utcOffsetMinutes) { return Copy.yesterday }
        let parts = day.split(separator: "-").map(String.init)
        guard parts.count == 3, let m = Int(parts[1]), let d = Int(parts[2]), (1 ... 12).contains(m) else { return day }
        let label = "\(d) \(Copy.months[m - 1])"
        return parts[0] == String(today.prefix(4)) ? label : "\(label) \(parts[0])"
    }

    /// Run sections (the Releases tab): In progress (running + queued, with
    /// a count), then one per local day, newest first.
    public static func runGroups(_ input: Input, filter: RunFilter, kinds: [RunKind]? = nil) -> [RunGroup] {
        let rows = runList(input, filter: filter, kinds: kinds)
        let isLive = { (row: RunListRow) in row.status == .running || row.status == .queued }
        let live = rows.filter(isLive)
        var groups: [RunGroup] = []
        if !live.isEmpty { groups.append(RunGroup(id: "in-progress", title: Copy.inProgress, count: live.count, rows: live)) }
        for row in rows where !isLive(row) {
            let day = localDay(msOf(row.at) ?? 0, input.utcOffsetMinutes)
            if let last = groups.indices.last, groups[last].id == day {
                groups[last].rows.append(row)
            } else {
                groups.append(RunGroup(id: day, title: dayTitle(day, now: input.now, utcOffsetMinutes: input.utcOffsetMinutes), count: nil, rows: [row]))
            }
        }
        return groups
    }

    /// The repository pick: `All repositories`, then every repository that
    /// has a run, by name.
    public static func repoOptions(_ input: Input) -> [Option] {
        var ids: [String] = []
        for run in input.runs where !ids.contains(run.repositoryId) { ids.append(run.repositoryId) }
        let named = ids.map { Option(label: repoName(input.repos, $0), value: $0) }
            .sorted { a, b in a.label != b.label ? jsLess(a.label, b.label) : jsLess(a.value, b.value) }
        return [Option(label: Copy.allRepositories, value: "all")] + named
    }

    private static func online(_ runner: Runner, _ now: Double) -> Bool {
        guard let seen = msOf(runner.lastSeenAt) else { return false }
        return now - seen < Limits.runnerOnlineMs
    }

    /// The toolbar's runner chip (it opens the Runners tab): `{online} of
    /// {total} runners`, a green dot while one is online. nil without runners.
    public static func runnerChip(_ input: Input) -> RunnerChip? {
        guard !input.runners.isEmpty else { return nil }
        let at = msOf(input.now) ?? 0
        let count = input.runners.filter { online($0, at) }.count
        return RunnerChip(
            label: Copy.runnerChip
                .replacingOccurrences(of: "{online}", with: String(count))
                .replacingOccurrences(of: "{total}", with: String(input.runners.count)),
            online: count > 0
        )
    }

    /// nav-badges.json destination `ci`: the viewer's runs, the newest per
    /// (repository, pipeline, ref): failed ones = `attention`, else the
    /// running / queued ones = `muted`, else none.
    public static func ciBadge(
        runs: [Run], jobs: [Job], pullRequests: [Pr], sessions: [Session], viewerId: String
    ) -> NavBadges.Value {
        var seen = Set<String>()
        var failed = 0
        var live = 0
        for run in newestFirst(runs) {
            guard isMine(run, viewerId: viewerId, pullRequests: pullRequests, sessions: sessions) else { continue }
            let key = "\(run.repositoryId)\u{0}\(run.name)\u{0}\(run.ref ?? "")"
            guard seen.insert(key).inserted else { continue }
            switch statusOf(run, jobs) {
            case .failed: failed += 1
            case .running, .queued: live += 1
            default: break
            }
        }
        return failed > 0 ? NavBadges.Value(count: failed, tone: .attention) : NavBadges.Value(count: live, tone: .muted)
    }

    public static func ciBadge(_ input: Input) -> NavBadges.Value {
        ciBadge(runs: input.runs, jobs: input.jobs, pullRequests: input.pullRequests, sessions: input.sessions, viewerId: input.viewerId)
    }

    /// A runner's labels: its OS, then its own, lower-cased.
    public static func runnerLabels(platform: String?, labels: [String]) -> [String] {
        var out: [String] = []
        for label in (RunnerSettings.osLabel(platform).map { [$0] } ?? []) + labels.map({ $0.lowercased() }) where !out.contains(label) {
            out.append(label)
        }
        return out
    }

    /// The Runners tab: online first, then by name.
    public static func runnerRows(_ input: Input) -> [RunnerRow] {
        let at = msOf(input.now) ?? 0
        let copy = Copy.Runners.self
        let rows = input.runners.map { runner -> RunnerRow in
            let labels = runnerLabels(platform: runner.platform, labels: runner.labels)
            let isOnline = online(runner, at)
            let busy = input.jobs.filter { $0.status == "in_progress" && $0.runnerDeviceId == runner.id }.count
            let queued = input.jobs.filter { job in
                source(job.provider) == .exponential && job.status == "queued" && job.runnerDeviceId == nil
                    && job.runsOn.allSatisfy { labels.contains($0.lowercased()) }
            }.count
            return RunnerRow(
                id: runner.id,
                title: runner.label,
                subtitle: labels.isEmpty ? copy.noLabels : labels.joined(separator: ", "),
                online: isOnline,
                busy: isOnline
                    ? copy.busy.replacingOccurrences(of: "{busy}", with: String(busy)).replacingOccurrences(of: "{slots}", with: String(runner.slots))
                    : nil,
                queue: queued > 0 ? copy.queue.replacingOccurrences(of: "{count}", with: String(queued)) : nil,
                offlineSince: isOnline ? nil : runner.lastSeenAt
            )
        }
        return rows.sorted { a, b in
            if a.online != b.online { return a.online }
            if a.title != b.title { return jsLess(a.title, b.title) }
            return jsLess(a.id, b.id)
        }
    }

    /// `/ci` (query + hash ignored): the CI page's path.
    public static func isPagePath(_ path: String) -> Bool {
        let bare = path.prefix { $0 != "#" && $0 != "?" }
        return bare.split(separator: "/").map(String.init) == ["ci"]
    }

    /// #EXP-2214 tab counts (after each label, muted; 0 = none): the merge
    /// queue's PRs (testing + up next), the Open pull requests, the release
    /// runs in progress, the runners.
    public static func tabCounts(_ input: Input) -> [Tab: Int] {
        [
            .train: queue(input).reduce(0) { $0 + $1.stats.inQueue },
            .prs: prGroups(input, filter: .all).first { $0.id == .open }?.count ?? 0,
            .releases: runGroups(input, filter: .all, kinds: [.release]).first { $0.id == "in-progress" }?.count ?? 0,
            .runners: input.runners.count,
        ]
    }
}
