import Foundation

/// #EXP-2172 (packages/domain-contract fixtures/ci-page.json is the spec; TS
/// reference `src/ci-page.ts`, desktop `domain::ci_page`, Android `CiPage`):
/// THE CI page (one root destination across every member team) and its
/// badge: our runs and GitHub Actions runs in one list, the merge train's
/// current batches, the runner slots. Pure, replayed by `CiPageTests`.
public enum CiPage {
    // MARK: - Copy, limits, test ids (fixture `copy` / `limits` / `testIds`)

    public enum Copy {
        public static let title = "CI"
        public static let tabs: [String: String] = ["runs": "Runs", "train": "Merge train", "runners": "Runners"]
        public static let filters: [String: String] = ["all": "All", "running": "Running", "failed": "Failed", "mine": "Mine"]
        public static let allRepositories = "All repositories"
        public static let sources: [String: String] = ["all": "Every source", "exponential": "Exponential", "github": "GitHub Actions"]
        public static let columns: [String: String] = [
            "status": "Status", "run": "Run", "repository": "Repository", "source": "Source",
            "ref": "Branch", "trigger": "Trigger", "duration": "Duration", "started": "Started",
        ]
        public static let trainSection = "Merge train"
        public static let pullRequest = "Pull request #{number}"
        public static let triggers: [String: String] = [
            "push": "Push", "pull_request": "Pull request", "tag": "Tag",
            "merge_train": "Merge train", "manual": "Manual", "schedule": "Schedule",
        ]
        public static let unknownRepository = "Repository"
        public static let inProgress = "In progress"
        public static let today = "Today"
        public static let yesterday = "Yesterday"
        public static let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        public static let queued = "queued"
        public static let batchTitle = "Testing {numbers} together"
        public static let runnerChip = "{online} of {total} runners"
        public static let waitingForRunner = "waiting for a runner"
        public static let trainBatch = "Testing {count} pull requests together"
        public enum Runners {
            public static let busy = "{busy} of {slots} busy"
            public static let queue = "{count} queued"
            public static let offlineSince = "Last seen {time}"
            public static let noLabels = "No labels"
        }
        public enum Empty {
            public static let runs = "No runs yet"
            public static let filtered = "No runs match"
            public static let train = "Nothing in the merge train"
            public static let runners = "No runners"
        }
    }

    public enum Limits {
        public static let runs = 100
        /// A runner is online while its last beat is younger than this.
        public static let runnerOnlineMs: Double = 120_000
    }

    public enum TestIds {
        public static let page = "ci-page"
        public static let tiles = "ci-tiles"
        public static let tabs = "ci-tabs"
        public static let filters = "ci-filters"
        public static let statusFilter = "ci-filter-status"
        public static let repoFilter = "ci-filter-repo"
        public static let sourceFilter = "ci-filter-source"
        public static let runs = "ci-runs"
        public static let runRow = "ci-run-row"
        public static let train = "ci-train"
        public static let trainRow = "ci-train-row"
        public static let runners = "ci-runners"
        public static let runnerRow = "ci-runner-row"
        public static let runnerChip = "ci-runner-chip"
    }

    // MARK: - Inputs

    public enum Source: String, Sendable, Equatable { case exponential, github }
    public enum Filter: String, Sendable, Equatable, CaseIterable { case all, running, failed, mine }
    public enum Tab: String, Sendable, Equatable, CaseIterable { case runs, train, runners }

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

        public init(
            id: String, repositoryId: String, number: Int, name: String, sha: String = "", ref: String?,
            trigger: String, state: String, pullRequestId: String?, createdByUserId: String?, provider: String,
            startedAt: String?, completedAt: String?, createdAt: String, title: String? = nil
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
        }
    }

    public struct Job: Equatable, Sendable {
        public let runId: String
        public let status: String
        public let conclusion: String?
        public let runsOn: [String]
        public let runnerDeviceId: String?
        public let provider: String

        public init(runId: String, status: String, conclusion: String?, runsOn: [String], runnerDeviceId: String?, provider: String) {
            self.runId = runId
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

        public init(id: String, fullName: String) {
            self.id = id
            self.fullName = fullName
        }
    }

    public struct Pr: Equatable, Sendable {
        public let id: String
        public let number: Int
        public let title: String
        public let repositoryId: String
        public let rootSessionId: String?
        public let base: String
        public let mergeState: String?
        public let mergePosition: Int?
        public let mergeTrainId: String?

        public init(
            id: String, number: Int, title: String, repositoryId: String, rootSessionId: String?, base: String,
            mergeState: String?, mergePosition: Int?, mergeTrainId: String?
        ) {
            self.id = id
            self.number = number
            self.title = title
            self.repositoryId = repositoryId
            self.rootSessionId = rootSessionId
            self.base = base
            self.mergeState = mergeState
            self.mergePosition = mergePosition
            self.mergeTrainId = mergeTrainId
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
        public var viewerId: String
        public var now: String
        /// The viewer's clock offset from UTC in minutes (Today / Yesterday).
        public var utcOffsetMinutes: Int

        public init(
            runs: [Run], jobs: [Job], repos: [Repo], pullRequests: [Pr], sessions: [Session], runners: [Runner],
            users: [User] = [], viewerId: String, now: String, utcOffsetMinutes: Int = 0
        ) {
            self.runs = runs
            self.jobs = jobs
            self.repos = repos
            self.pullRequests = pullRequests
            self.sessions = sessions
            self.runners = runners
            self.users = users
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
        /// The commit / PR line (`title`), else the merge train batch, else
        /// the pipeline's name.
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

    public struct TrainRow: Equatable, Sendable {
        public let id: String
        public let identifier: String
        public let title: String
        public let subtitle: String?
        public let glyph: PrChecks.Glyph
        public let badge: String
        /// The PR a press opens (a batch: its first member).
        public let pullRequestId: String
    }

    public struct TrainGroup: Equatable, Sendable {
        public let repositoryId: String
        public let title: String
        public let caption: String
        public let rows: [TrainRow]
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

    private static func msOf(_ value: String?) -> Double? { CiRun.ms(value) }

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
    private static func newestFirst(_ runs: [Run]) -> [Run] {
        runs.sorted { a, b in
            let (x, y) = (runAt(a), runAt(b))
            return x != y ? x > y : jsLess(a.id, b.id)
        }
    }

    private static func statusOf(_ run: Run, _ jobs: [Job]) -> CiRun.Status {
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

    private static func triggerText(_ run: Run, _ pullRequests: [Pr]) -> String {
        if run.trigger == "pull_request", let prId = run.pullRequestId, let pr = pullRequests.first(where: { $0.id == prId }) {
            return Copy.pullRequest.replacingOccurrences(of: "{number}", with: String(pr.number))
        }
        return Copy.triggers[run.trigger] ?? run.trigger
    }

    public static func repoName(_ repos: [Repo], _ id: String) -> String {
        repos.first { $0.id == id }?.fullName ?? Copy.unknownRepository
    }

    /// A running merge train run is its batch: `Testing #a, #b together`.
    private static func batchTitle(_ run: Run, _ pullRequests: [Pr]) -> String? {
        guard run.trigger == "merge_train", run.state == "pending" else { return nil }
        let members = pullRequests.filter { $0.repositoryId == run.repositoryId && $0.mergeState == "testing" }.map(\.number).sorted()
        guard !members.isEmpty else { return nil }
        return Copy.batchTitle.replacingOccurrences(of: "{numbers}", with: members.map { "#\($0)" }.joined(separator: ", "))
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
        return RunListRow(
            id: run.id,
            status: status,
            statusLabel: CiRun.statusLabel(status),
            glyph: glyph,
            tone: tone,
            title: run.title ?? batchTitle(run, input.pullRequests) ?? pr?.title ?? run.name,
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

    /// The Runs tab: every run of the filter, newest first, at most `Limits.runs`.
    public static func runList(_ input: Input, filter: RunFilter) -> [RunListRow] {
        var rows: [RunListRow] = []
        for run in newestFirst(input.runs) {
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

    /// The Runs tab's sections: In progress (running + queued, with a
    /// count), then one per local day, newest first.
    public static func runGroups(_ input: Input, filter: RunFilter) -> [RunGroup] {
        let rows = runList(input, filter: filter)
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

    private static let trainWaiting: Set<String> = ["queued", "checks", "held", "conflict", "failing"]

    /// The merge train per repository: its testing batches, then (`queue`:
    /// the Merge train tab) the queued PRs by position and the waiting ones.
    public static func trainGroups(_ input: Input, queue: Bool) -> [TrainGroup] {
        let pill = MergeTrain.Copy.Pill.self
        let waiting: [String: String] = [
            "checks": pill.checksRunning, "held": pill.onHold, "conflict": pill.conflict, "failing": pill.failing,
        ]
        var order: [String] = []
        var byRepo: [String: [Pr]] = [:]
        for pr in input.pullRequests {
            let state = pr.mergeState
            guard state == "testing" || (queue && state.map { trainWaiting.contains($0) } == true) else { continue }
            if byRepo[pr.repositoryId] == nil { order.append(pr.repositoryId) }
            byRepo[pr.repositoryId, default: []].append(pr)
        }
        var groups: [TrainGroup] = []
        for repositoryId in order {
            let prs = byRepo[repositoryId] ?? []
            var rows: [TrainRow] = []
            var batchOrder: [String] = []
            var batches: [String: [Pr]] = [:]
            let testing = prs.enumerated().filter { $0.element.mergeState == "testing" }
                .sorted { $0.element.number != $1.element.number ? $0.element.number < $1.element.number : $0.offset < $1.offset }
                .map(\.element)
            for pr in testing {
                let key = pr.mergeTrainId ?? pr.id
                if batches[key] == nil { batchOrder.append(key) }
                batches[key, default: []].append(pr)
            }
            let sortedBatches = batchOrder.enumerated().sorted { a, b in
                let (x, y) = (batches[a.element]![0].number, batches[b.element]![0].number)
                return x != y ? x < y : a.offset < b.offset
            }.map(\.element)
            for key in sortedBatches {
                let members = batches[key]!
                let first = members[0]
                rows.append(TrainRow(
                    id: "batch.\(key)",
                    identifier: "#\(first.number)",
                    title: members.count == 1 ? first.title : Copy.trainBatch.replacingOccurrences(of: "{count}", with: String(members.count)),
                    subtitle: members.count == 1 ? nil : members.map { "#\($0.number) \($0.title)" }.joined(separator: " · "),
                    glyph: .running,
                    badge: pill.testing,
                    pullRequestId: first.id
                ))
            }
            if queue {
                let queued = prs.enumerated().filter { $0.element.mergeState == "queued" }.sorted { a, b in
                    let (x, y) = (a.element.mergePosition ?? Int.max, b.element.mergePosition ?? Int.max)
                    if x != y { return x < y }
                    if a.element.number != b.element.number { return a.element.number < b.element.number }
                    return a.offset < b.offset
                }.map(\.element)
                for pr in queued {
                    let badge = pr.mergePosition.map {
                        pill.queuedAt.replacingOccurrences(of: "{position}", with: MergeTrain.ordinal($0))
                    } ?? pill.queued
                    rows.append(TrainRow(
                        id: pr.id, identifier: "#\(pr.number)", title: pr.title, subtitle: nil,
                        glyph: .queued, badge: badge, pullRequestId: pr.id
                    ))
                }
                let rest = prs.enumerated().filter { $0.element.mergeState != "queued" && $0.element.mergeState != "testing" }
                    .sorted { $0.element.number != $1.element.number ? $0.element.number < $1.element.number : $0.offset < $1.offset }
                    .map(\.element)
                for pr in rest {
                    let state = pr.mergeState ?? ""
                    let glyph: PrChecks.Glyph = state == "checks" ? .queued : state == "held" ? .skipped : .failed
                    rows.append(TrainRow(
                        id: pr.id, identifier: "#\(pr.number)", title: pr.title, subtitle: nil,
                        glyph: glyph, badge: waiting[state] ?? "", pullRequestId: pr.id
                    ))
                }
            }
            guard !rows.isEmpty else { continue }
            groups.append(TrainGroup(
                repositoryId: repositoryId, title: repoName(input.repos, repositoryId), caption: prs[0].base, rows: rows
            ))
        }
        return groups.sorted { a, b in a.title != b.title ? jsLess(a.title, b.title) : jsLess(a.repositoryId, b.repositoryId) }
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

    /// Tab counts: the train's rows, the runners.
    public static func tabCounts(_ input: Input) -> (train: Int, runners: Int) {
        (trainGroups(input, queue: true).reduce(0) { $0 + $1.rows.count }, input.runners.count)
    }
}

extension CiPage {
    // MARK: - The specimen (fixture `scenario`: the page every platform's shot renders)

    public static let specimen = Input(
        runs: [
            Run(id: "r483", repositoryId: "repo-engine", number: 483, name: "gate", sha: "82aa98f382aa98f382aa98f382aa98f382aa98f3", ref: "exp/APP-32", trigger: "pull_request", state: "pending", pullRequestId: "pr-214", createdByUserId: nil, provider: "exponential", startedAt: "2026-11-27T10:26:48.000Z", completedAt: nil, createdAt: "2026-11-27T10:26:48.000Z", title: "Repack once per push, not per ref"),
            Run(id: "r91", repositoryId: "repo-engine", number: 91, name: "Merge train", sha: "3db841eb3db841eb3db841eb3db841eb3db841eb", ref: "main", trigger: "merge_train", state: "pending", pullRequestId: nil, createdByUserId: nil, provider: "exponential", startedAt: "2026-11-27T10:28:20.000Z", completedAt: nil, createdAt: "2026-11-27T10:28:20.000Z", title: nil),
            Run(id: "g1207", repositoryId: "repo-mobile", number: 1207, name: "build-ios", sha: "f78ec187f78ec187f78ec187f78ec187f78ec187", ref: "release/2.4", trigger: "tag", state: "pending", pullRequestId: nil, createdByUserId: "u-me", provider: "github", startedAt: nil, completedAt: nil, createdAt: "2026-11-27T10:29:00.000Z", title: "Bump the widget to 2.4.1"),
            Run(id: "r482", repositoryId: "repo-engine", number: 482, name: "gate", sha: "e4731f42e4731f42e4731f42e4731f42e4731f42", ref: "exp/APP-33", trigger: "pull_request", state: "failure", pullRequestId: "pr-213", createdByUserId: nil, provider: "exponential", startedAt: "2026-11-27T09:59:20.000Z", completedAt: "2026-11-27T10:12:00.000Z", createdAt: "2026-11-27T09:59:20.000Z", title: "Mirror prunes deleted branches"),
            Run(id: "g1206", repositoryId: "repo-mobile", number: 1206, name: "test", sha: "595747d6595747d6595747d6595747d6595747d6", ref: "main", trigger: "push", state: "success", pullRequestId: nil, createdByUserId: nil, provider: "github", startedAt: "2026-11-27T09:22:54.000Z", completedAt: "2026-11-27T09:30:00.000Z", createdAt: "2026-11-27T09:22:54.000Z", title: "Offline queue survives a cold start"),
            Run(id: "g57", repositoryId: "repo-site", number: 57, name: "deploy", sha: "3a5a18693a5a18693a5a18693a5a18693a5a1869", ref: "main", trigger: "push", state: "failure", pullRequestId: nil, createdByUserId: "u-me", provider: "github", startedAt: "2026-11-26T08:27:57.000Z", completedAt: "2026-11-26T08:30:00.000Z", createdAt: "2026-11-26T08:27:57.000Z", title: "Pricing page copy"),
            Run(id: "g58", repositoryId: "repo-site", number: 58, name: "deploy", sha: "d891921ad891921ad891921ad891921ad891921a", ref: "main", trigger: "push", state: "success", pullRequestId: nil, createdByUserId: "u-me", provider: "github", startedAt: "2026-11-27T08:27:57.000Z", completedAt: "2026-11-27T08:30:00.000Z", createdAt: "2026-11-27T08:27:57.000Z", title: "Pricing page copy, take two"),
            Run(id: "r481", repositoryId: "repo-engine", number: 481, name: "gate", sha: "463ba591463ba591463ba591463ba591463ba591", ref: "exp/APP-32", trigger: "pull_request", state: "failure", pullRequestId: "pr-214", createdByUserId: nil, provider: "exponential", startedAt: "2026-11-27T07:30:00.000Z", completedAt: "2026-11-27T07:30:24.000Z", createdAt: "2026-11-27T07:30:00.000Z", title: "Repack once per push, not per ref"),
            Run(id: "r480", repositoryId: "repo-gone", number: 480, name: "nightly", sha: "a8042be0a8042be0a8042be0a8042be0a8042be0", ref: nil, trigger: "schedule", state: "success", pullRequestId: nil, createdByUserId: nil, provider: "exponential", startedAt: "2026-11-27T01:30:00.000Z", completedAt: "2026-11-27T01:44:12.000Z", createdAt: "2026-11-27T01:30:00.000Z", title: nil),
        ],
        jobs: [
            Job(runId: "r483", status: "in_progress", conclusion: nil, runsOn: ["linux"], runnerDeviceId: "dev-mint", provider: "exponential"),
            Job(runId: "r483", status: "queued", conclusion: nil, runsOn: ["linux"], runnerDeviceId: nil, provider: "exponential"),
            Job(runId: "r483", status: "queued", conclusion: nil, runsOn: ["mac"], runnerDeviceId: nil, provider: "exponential"),
            Job(runId: "r91", status: "in_progress", conclusion: nil, runsOn: ["linux"], runnerDeviceId: "dev-mint", provider: "exponential"),
            Job(runId: "g1207", status: "queued", conclusion: nil, runsOn: ["macos-latest"], runnerDeviceId: nil, provider: "github"),
            Job(runId: "r482", status: "completed", conclusion: "success", runsOn: [], runnerDeviceId: nil, provider: "exponential"),
            Job(runId: "r482", status: "completed", conclusion: "failure", runsOn: [], runnerDeviceId: nil, provider: "exponential"),
            Job(runId: "g1206", status: "completed", conclusion: "success", runsOn: [], runnerDeviceId: nil, provider: "github"),
            Job(runId: "g57", status: "completed", conclusion: "failure", runsOn: [], runnerDeviceId: nil, provider: "github"),
            Job(runId: "g58", status: "completed", conclusion: "success", runsOn: [], runnerDeviceId: nil, provider: "github"),
            Job(runId: "r481", status: "completed", conclusion: "cancelled", runsOn: [], runnerDeviceId: nil, provider: "exponential"),
            Job(runId: "r480", status: "completed", conclusion: "success", runsOn: [], runnerDeviceId: nil, provider: "exponential"),
        ],
        repos: [
            Repo(id: "repo-engine", fullName: "acme/engine"),
            Repo(id: "repo-mobile", fullName: "acme/mobile"),
            Repo(id: "repo-site", fullName: "acme/site"),
        ],
        pullRequests: [
            Pr(id: "pr-211", number: 211, title: "Fix bisect on a red batch", repositoryId: "repo-engine", rootSessionId: "s-other", base: "main", mergeState: "testing", mergePosition: nil, mergeTrainId: "train-91"),
            Pr(id: "pr-212", number: 212, title: "Runner labels", repositoryId: "repo-engine", rootSessionId: "s-other", base: "main", mergeState: "testing", mergePosition: nil, mergeTrainId: "train-91"),
            Pr(id: "pr-213", number: 213, title: "Log tail", repositoryId: "repo-engine", rootSessionId: "s-mine", base: "main", mergeState: "testing", mergePosition: nil, mergeTrainId: "train-91"),
            Pr(id: "pr-214", number: 214, title: "Reduce cold start below 400 ms", repositoryId: "repo-engine", rootSessionId: "s-mine", base: "main", mergeState: "checks", mergePosition: nil, mergeTrainId: nil),
            Pr(id: "pr-215", number: 215, title: "Cache the planner", repositoryId: "repo-engine", rootSessionId: "s-other", base: "main", mergeState: "queued", mergePosition: 1, mergeTrainId: nil),
            Pr(id: "pr-40", number: 40, title: "Dark splash", repositoryId: "repo-mobile", rootSessionId: nil, base: "main", mergeState: "held", mergePosition: nil, mergeTrainId: nil),
            Pr(id: "pr-41", number: 41, title: "Old", repositoryId: "repo-mobile", rootSessionId: nil, base: "main", mergeState: "merged", mergePosition: nil, mergeTrainId: nil),
        ],
        sessions: [
            Session(id: "s-mine", userId: "u-me"),
            Session(id: "s-other", userId: "u-other"),
        ],
        runners: [
            Runner(id: "dev-mint", label: "mint", platform: "linux", labels: ["GPU"], slots: 2, lastSeenAt: "2026-11-27T10:29:40.000Z"),
            Runner(id: "dev-mac", label: "Alex's MacBook Pro", platform: "macos", labels: [], slots: 1, lastSeenAt: "2026-11-27T10:29:55.000Z"),
            Runner(id: "dev-old", label: "build-box", platform: nil, labels: [], slots: 4, lastSeenAt: "2026-11-26T22:00:00.000Z"),
        ],
        users: [
            User(id: "u-me", name: "Mara Lind", image: nil),
            User(id: "u-other", name: "Jonas Berg", image: nil),
        ],
        viewerId: "u-me",
        now: "2026-11-27T10:30:00.000Z",
        utcOffsetMinutes: 60
    )
}
