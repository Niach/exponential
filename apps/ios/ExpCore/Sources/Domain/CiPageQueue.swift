import Foundation

/// #EXP-2214 (fixtures/ci-page.json `prGroupsCases`, `queueCases`,
/// `etaCases`, `typicalTrainMs`, `runContextCases`; TS reference
/// `src/ci-page.ts`): the Pull requests tab, the Merge queue tab and the run
/// page's context. Pure, replayed by `CiPageTests`.
extension CiPage {
    // MARK: - Pull requests tab

    public struct Pill: Equatable, Sendable {
        public let text: String
        /// neutral | info | success | danger.
        public let tone: String
    }

    /// One subject of the Pull requests tab: a pull request (its runs), or a
    /// branch without one (`Branches`).
    public struct SubjectRow: Equatable, Sendable {
        /// The PR's id, else `branch.<repositoryId>.<ref>`.
        public let id: String
        public let pullRequestId: String?
        /// `#214`; nil for a branch.
        public let identifier: String?
        public let title: String
        public let status: CiRun.Status
        public let glyph: PrChecks.Glyph
        public let tone: String
        /// Wide: `repo · ref · name #n` of the run that decides the status.
        public let subtitle: String
        /// Phones: `repo · name #n`.
        public let phoneSubtitle: String
        /// That run's duration, else copy.queued.
        public let meta: String
        /// The PR's merge queue state (wide only); nil for a branch.
        public let pill: Pill?
        /// The run a press opens (the deciding one).
        public let runId: String
        /// Its runs, newest first, at most `Limits.subjectRuns` (folded).
        public let runs: [RunListRow]
    }

    public enum SubjectGroupId: String, Sendable, Equatable { case open, branches, closed }

    public struct SubjectGroup: Equatable, Sendable {
        public let id: SubjectGroupId
        public let title: String
        public let count: Int
        /// Merged or closed starts folded.
        public let open: Bool
        public let rows: [SubjectRow]
    }

    static let statusRank: [CiRun.Status] = [.failed, .running, .queued, .passed, .cancelled]

    /// A PR's merge queue state as a pill (merge-train.json copy.pill).
    public static func mergePill(_ pr: Pr) -> Pill? {
        let pill = MergeTrain.Copy.Pill.self
        switch pr.mergeState {
        case "testing": return Pill(text: pill.testing, tone: "info")
        case "queued":
            let position = pr.mergePosition.flatMap { $0 != 0 ? $0 : nil }
            return Pill(text: position.map { pill.queuedAt.replacingOccurrences(of: "{position}", with: MergeTrain.ordinal($0)) } ?? pill.queued, tone: "neutral")
        case "merged": return Pill(text: pill.merged, tone: "success")
        case "conflict": return Pill(text: pill.conflict, tone: "danger")
        case "failing": return Pill(text: pill.failing, tone: "danger")
        case "held": return Pill(text: pill.onHold, tone: "neutral")
        case "draft": return Pill(text: pill.draft, tone: "neutral")
        case "checks":
            let wait = present(pr.mergeWait)
            return Pill(
                text: wait.flatMap { Copy.Queue.waits[$0] } ?? pill.checksRunning,
                tone: pr.mergeWait == "failing" ? "danger" : (pr.mergeWait ?? "checks") == "checks" ? "info" : "neutral"
            )
        default: return nil
        }
    }

    /// THE PULL REQUESTS TAB: pull request runs grouped by PR, branch runs by
    /// (repository, ref). A subject's status rolls up the NEWEST run of each
    /// pipeline name (failed, running, queued, passed, cancelled: the first
    /// present wins; the newest run with it decides the row's subtitle, meta
    /// and press). Sections: Open, Branches, Merged or closed (folded); each
    /// newest first, at most `Limits.subjects`; an empty one is left out.
    public static func prGroups(_ input: Input, filter: RunFilter) -> [SubjectGroup] {
        var order: [String] = []
        var subjects: [String: (pr: Pr?, ref: String?, runs: [Run])] = [:]
        for run in newestFirst(input.runs) {
            let kind = runKind(run, repos: input.repos)
            guard kind == .pullRequest || kind == .branch else { continue }
            if let repo = present(filter.repositoryId), run.repositoryId != repo { continue }
            let key = run.pullRequestId ?? "branch.\(run.repositoryId).\(run.ref ?? "")"
            if subjects[key] == nil {
                let pr = run.pullRequestId.flatMap { id in input.pullRequests.first { $0.id == id } }
                subjects[key] = (pr, run.ref, [])
                order.append(key)
            }
            subjects[key]!.runs.append(run)
        }
        var groups: [SubjectGroupId: [SubjectRow]] = [.open: [], .branches: [], .closed: []]
        for key in order {
            let subject = subjects[key]!
            var names: [String] = []
            var newestPerName: [String: Run] = [:]
            for run in subject.runs where newestPerName[run.name] == nil {
                newestPerName[run.name] = run
                names.append(run.name)
            }
            let latest = names.map { name -> (run: Run, status: CiRun.Status) in
                let run = newestPerName[name]!
                return (run, statusOf(run, input.jobs))
            }
            let status = statusRank.first { s in latest.contains { $0.status == s } } ?? .queued
            let deciding = latest.first { $0.status == status }!.run
            if filter.status == .running, status != .running, status != .queued { continue }
            if filter.status == .failed, status != .failed { continue }
            if filter.status == .mine,
               !subject.runs.contains(where: { isMine($0, viewerId: input.viewerId, pullRequests: input.pullRequests, sessions: input.sessions) }) { continue }
            let row = runRow(deciding, input)
            let prId = subject.runs[0].pullRequestId
            let branch = prId == nil
            let group: SubjectGroupId = branch
                ? .branches
                : subject.pr.map { $0.state == "merged" || $0.state == "closed" } == true ? .closed : .open
            if groups[group]!.count >= Limits.subjects { continue }
            let (glyph, tone) = statusTone(status)
            groups[group]!.append(SubjectRow(
                id: key,
                pullRequestId: subject.pr?.id ?? prId,
                identifier: subject.pr.map { "#\($0.number)" },
                title: subject.pr?.title ?? (branch ? (subject.ref ?? RepoCi.Copy.Runs.noRef) : row.title),
                status: status,
                glyph: glyph,
                tone: tone,
                subtitle: [row.repo, row.ref, row.workflow].joined(separator: " · "),
                phoneSubtitle: [row.repo, row.workflow].joined(separator: " · "),
                meta: row.phoneMeta,
                pill: subject.pr.flatMap(mergePill),
                runId: deciding.id,
                runs: subject.runs.prefix(Limits.subjectRuns).map { runRow($0, input) }
            ))
        }
        let sections: [(SubjectGroupId, String, Bool)] = [
            (.open, Copy.Prs.open, true), (.branches, Copy.Prs.branches, true), (.closed, Copy.Prs.closed, false),
        ]
        return sections.compactMap { id, title, open in
            let rows = groups[id]!
            return rows.isEmpty ? nil : SubjectGroup(id: id, title: title, count: rows.count, open: open, rows: rows)
        }
    }

    // MARK: - Merge queue tab

    static let liveTrain: Set<String> = ["dispatching", "composing", "testing", "green"]

    public struct QueueMember: Equatable, Sendable {
        public let pullRequestId: String
        public let identifier: String
        public let title: String
        public let author: User?
    }

    /// The run testing now (one Card).
    public struct QueueNow: Equatable, Sendable {
        public let trainId: String
        /// The PR's title, else `Testing #a, #b together`.
        public let title: String
        /// `main @ 3db841e` (+ ` · re-run`, ` · split: …`, ` · Merge now, untested`).
        public let subtitle: String
        public let members: [QueueMember]
        public let status: CiRun.Status
        /// The run Open run opens; nil before one exists.
        public let runId: String?
        /// `Running for 2m 40s`, else copy.waitingForRunner.
        public let elapsed: String
        /// `about 3 min left` | copy.queue.almostDone; nil without history.
        public let eta: String?
        /// 0–99 against the typical run; nil without history.
        public let progress: Int?
    }

    public enum QueueAction: String, Sendable, Equatable { case openLog = "open-log", resume }

    public struct QueueRow: Equatable, Sendable {
        public let id: String
        public let pullRequestId: String
        public let identifier: String
        public let title: String
        public let subtitle: String?
        public let glyph: String
        /// neutral | success | danger | warning.
        public let tone: String
        /// A pill (Not ready yet).
        public let badge: Pill?
        /// History rows: when it happened (drawn relative).
        public let at: String?
        public let action: QueueAction?
        public let runId: String?
        public let jobId: String?
        public let url: String?
        /// Up next: the ⋯ menu (Merge now, Hold).
        public let menu: Bool

        init(
            id: String, pullRequestId: String, identifier: String, title: String, subtitle: String?, glyph: String,
            tone: String, badge: Pill? = nil, at: String? = nil, action: QueueAction? = nil, runId: String? = nil,
            jobId: String? = nil, url: String? = nil, menu: Bool = false
        ) {
            self.id = id
            self.pullRequestId = pullRequestId
            self.identifier = identifier
            self.title = title
            self.subtitle = subtitle
            self.glyph = glyph
            self.tone = tone
            self.badge = badge
            self.at = at
            self.action = action
            self.runId = runId
            self.jobId = jobId
            self.url = url
            self.menu = menu
        }
    }

    public struct QueueStats: Equatable, Sendable {
        public let inQueue: Int
        public let landedToday: Int
        public let sentBackToday: Int
        /// The typical (median) merge queue run; nil without enough history.
        public let typicalRun: String?
    }

    public struct QueueRepo: Equatable, Sendable {
        public let repositoryId: String
        public let title: String
        /// copy.queue.into over the base (`into main`).
        public let caption: String
        public let stats: QueueStats
        public let testing: [QueueNow]
        public let upNext: [QueueRow]
        public let waiting: [QueueRow]
        public let sentBack: [QueueRow]
        public let landed: [QueueRow]
    }

    /// The finished green merge queue runs of a repository: per train, the
    /// span of its CI runs (first start → last completion), newest first, at
    /// most `Limits.etaSamples * 2`.
    private static func trainDurations(_ input: Input, _ repositoryId: String) -> [Double] {
        var order: [String] = []
        var byTrain: [String: [Run]] = [:]
        for run in input.runs where run.repositoryId == repositoryId {
            guard let trainId = present(run.mergeTrainId) else { continue }
            if byTrain[trainId] == nil { order.append(trainId) }
            byTrain[trainId, default: []].append(run)
        }
        var spans: [(end: Double, ms: Double)] = []
        for trainId in order {
            let runs = byTrain[trainId]!
            guard runs.allSatisfy({ statusOf($0, input.jobs) == .passed }) else { continue }
            let starts = runs.map { msOf($0.startedAt) }
            let ends = runs.map { msOf($0.completedAt) }
            guard !starts.contains(where: { $0 == nil }), !ends.contains(where: { $0 == nil }) else { continue }
            let start = starts.compactMap { $0 }.min()!
            let end = ends.compactMap { $0 }.max()!
            spans.append((end, end - start))
        }
        return stableSorted(spans) { a, b in a.end > b.end ? -1 : a.end < b.end ? 1 : 0 }
            .prefix(Limits.etaSamples * 2)
            .map(\.ms)
    }

    /// The median of the repository's finished green merge queue runs (the
    /// last `etaSamples * 2`), nil with fewer than `etaSamples`.
    public static func typicalTrainMs(_ input: Input, repositoryId: String) -> Double? {
        let spans = trainDurations(input, repositoryId).sorted()
        guard spans.count >= Limits.etaSamples else { return nil }
        let mid = spans.count / 2
        return spans.count % 2 == 1 ? spans[mid] : ((spans[mid - 1] + spans[mid]) / 2).rounded(.toNearestOrAwayFromZero)
    }

    /// `trainEta` (fixture `etaCases`): what is left of a run going
    /// `elapsedMs` against the typical run: `about {n} min left` while more
    /// than a minute is left, else copy.queue.almostDone; progress 0–99.
    public static func trainEta(typicalMs: Double?, elapsedMs: Double) -> (eta: String, progress: Int)? {
        guard let typicalMs, typicalMs > 0 else { return nil }
        let left = typicalMs - elapsedMs
        let eta = left > 60_000
            ? Copy.Queue.left.replacingOccurrences(of: "{n}", with: String(Int((left / 60_000).rounded(.up))))
            : Copy.Queue.almostDone
        return (eta, max(0, min(99, Int(((elapsedMs / typicalMs) * 100).rounded(.down)))))
    }

    private static func authorOf(_ pr: Pr, _ input: Input) -> User? {
        guard let root = pr.rootSessionId, let starter = input.sessions.first(where: { $0.id == root })?.userId else { return nil }
        return input.users.first { $0.id == starter }
    }

    static func listOf(_ items: [String], _ max: Int) -> String {
        items.count <= max ? items.joined(separator: ", ") : "\(items.prefix(max).joined(separator: ", ")) +\(items.count - max)"
    }

    static func tries(_ n: Int) -> String {
        Copy.Queue.tries.replacingOccurrences(of: "{n}", with: String(n)).replacingOccurrences(of: "{max}", with: String(MergeTrain.Limits.bounces))
    }

    private static func baseLine(_ train: Train) -> String {
        guard let sha = present(train.baseSha) else { return train.base }
        return Copy.Queue.base.replacingOccurrences(of: "{base}", with: train.base).replacingOccurrences(of: "{sha}", with: String(sha.prefix(7)))
    }

    private static func fileName(_ path: String) -> String {
        path.split(separator: "/", omittingEmptySubsequences: false).last.map(String.init) ?? path
    }

    /// THE MERGE QUEUE TAB (fixture `queueCases`; the TS `ciQueue` doc
    /// comment is the rule), per repository by name: Testing now, Up next,
    /// Not ready yet, Sent back, Landed and the stats.
    public static func queue(_ input: Input) -> [QueueRepo] {
        let copy = Copy.Queue.self
        let now = msOf(input.now) ?? 0
        let since = now - Limits.historyHours * 3_600_000
        let today = localDay(now, input.utcOffsetMinutes)
        let prOf = { (id: String) in input.pullRequests.first { $0.id == id } }
        var repoIds: [String] = []
        func add(_ id: String) { if !repoIds.contains(id) { repoIds.append(id) } }
        for t in input.trains where liveTrain.contains(t.state) && present(t.finishedAt) == nil { add(t.repositoryId) }
        for pr in input.pullRequests where pr.state == "open" && ["queued", "testing", "held", "conflict", "failing"].contains(pr.mergeState ?? "") {
            add(pr.repositoryId)
        }
        for e in input.trainEvents where (msOf(e.createdAt) ?? 0) >= since { add(e.repositoryId) }
        var out: [QueueRepo] = []
        for repositoryId in repoIds {
            let typical = typicalTrainMs(input, repositoryId: repositoryId)
            let live = stableSorted(input.trains.filter { $0.repositoryId == repositoryId && liveTrain.contains($0.state) && present($0.finishedAt) == nil }) { a, b in
                let (x, y) = (msOf(a.createdAt) ?? 0, msOf(b.createdAt) ?? 0)
                return x != y ? (x < y ? -1 : 1) : jsLess(a.id, b.id) ? -1 : 1
            }
            let testing: [QueueNow] = live.map { train in
                let members = train.units.flatMap { $0 }.compactMap { id in
                    prOf(id).map { QueueMember(pullRequestId: $0.id, identifier: "#\($0.number)", title: $0.title, author: authorOf($0, input)) }
                }
                let runs = newestFirst(input.runs.filter { $0.mergeTrainId == train.id })
                let statuses = runs.map { (run: $0, status: statusOf($0, input.jobs)) }
                let status = statusRank.first { s in statuses.contains { $0.status == s } } ?? .queued
                let deciding = statuses.first { $0.status == status }?.run
                let started = runs.compactMap { msOf($0.startedAt) }.min()
                let eta = started.flatMap { trainEta(typicalMs: typical, elapsedMs: now - $0) }
                var parts = [baseLine(train)]
                if train.attempt > 0 { parts.append(copy.rerun) }
                if present(train.bisectOf) != nil, !train.pending.isEmpty {
                    parts.append(copy.split.replacingOccurrences(of: "{count}", with: String(train.pending.count)))
                }
                if train.untested { parts.append(copy.untested) }
                return QueueNow(
                    trainId: train.id,
                    title: members.count == 1
                        ? members[0].title
                        : Copy.batchTitle.replacingOccurrences(of: "{numbers}", with: members.map(\.identifier).joined(separator: ", ")),
                    subtitle: parts.joined(separator: " · "),
                    members: members,
                    status: status,
                    runId: deciding?.id,
                    elapsed: started.map { copy.runningFor.replacingOccurrences(of: "{duration}", with: PrChecks.formatDuration(ms: now - $0)) }
                        ?? Copy.waitingForRunner,
                    eta: eta?.eta,
                    progress: eta?.progress
                )
            }
            let open = input.pullRequests.filter { $0.repositoryId == repositoryId && $0.state == "open" }
            let positionOf = { (pr: Pr) in pr.mergePosition ?? Int.max }
            let upNext: [QueueRow] = stableSorted(open.filter { $0.mergeState == "queued" }) { a, b in
                positionOf(a) != positionOf(b) ? (positionOf(a) < positionOf(b) ? -1 : 1) : a.number - b.number
            }.map { pr in
                let position = pr.mergePosition.flatMap { $0 != 0 ? $0 : nil }
                return QueueRow(
                    id: pr.id, pullRequestId: pr.id, identifier: "#\(pr.number)", title: pr.title,
                    subtitle: pr.mergeNow ? copy.mergeNow : position.map { copy.position.replacingOccurrences(of: "{ordinal}", with: MergeTrain.ordinal($0)) },
                    glyph: pr.mergeNow ? "zap" : "ci-queued",
                    tone: pr.mergeNow ? "warning" : "neutral",
                    menu: true
                )
            }
            let waitingStates: Set<String> = ["checks", "held", "conflict", "failing"]
            let waiting: [QueueRow] = stableSorted(open.filter { waitingStates.contains($0.mergeState ?? "") }) { $0.number - $1.number }.map { pr in
                let state = pr.mergeState ?? ""
                let held = state == "held"
                let text = held
                    ? (copy.holds[pr.mergeHold ?? "person"] ?? copy.holds["person"]!)
                    : state == "checks" ? (copy.waits[pr.mergeWait ?? "checks"] ?? copy.waits["checks"]!) : (copy.waits[state] ?? "")
                let tone = state == "conflict" || state == "failing" || (state == "checks" && pr.mergeWait == "failing")
                    ? "danger"
                    : state == "checks" && (pr.mergeWait ?? "checks") == "checks" ? "info" : "neutral"
                return QueueRow(
                    id: pr.id, pullRequestId: pr.id, identifier: "#\(pr.number)", title: pr.title, subtitle: nil,
                    glyph: held ? "ci-skipped" : tone == "danger" ? "ci-failed" : "ci-queued",
                    tone: tone == "info" ? "neutral" : tone,
                    badge: Pill(text: text, tone: tone),
                    action: held && pr.mergeHold != nil && pr.mergeHold != "person" ? .resume : nil
                )
            }
            let events = stableSorted(input.trainEvents.filter { $0.repositoryId == repositoryId && (msOf($0.createdAt) ?? 0) >= since }) { a, b in
                let (x, y) = (msOf(a.createdAt) ?? 0, msOf(b.createdAt) ?? 0)
                return x != y ? (y < x ? -1 : 1) : jsLess(a.id, b.id) ? -1 : 1
            }
            let sentBack: [QueueRow] = events.filter { $0.kind == "failing" || $0.kind == "conflict" }.prefix(Limits.history).compactMap { e in
                guard let pr = prOf(e.pullRequestId) else { return nil }
                let job = present(e.ciJobId).flatMap { id in input.jobs.first { $0.id == id } }
                let subtitle = e.kind == "failing"
                    ? "\(copy.failedIn.replacingOccurrences(of: "{checks}", with: listOf(e.checks, 2))) · \(tries(e.attempt))"
                    : "\(copy.conflictIn.replacingOccurrences(of: "{files}", with: listOf(e.files.map(fileName), 2))) · \(tries(e.attempt))"
                let action: QueueAction? = e.kind == "failing" && (job != nil || present(e.jobUrl) != nil) ? .openLog : nil
                return QueueRow(
                    id: e.id, pullRequestId: pr.id, identifier: "#\(pr.number)", title: pr.title, subtitle: subtitle,
                    glyph: e.kind == "conflict" ? "pr-conflict" : "ci-failed", tone: "danger",
                    at: e.createdAt, action: action,
                    runId: action == .openLog ? job?.runId : nil,
                    jobId: action == .openLog ? job?.id : nil,
                    url: action == .openLog && job == nil ? e.jobUrl : nil
                )
            }
            // A PR sent back in the window is listed there, not again here.
            let sentBackPrs = Set(sentBack.map(\.pullRequestId))
            let notReady = waiting.filter { row in
                guard let pr = prOf(row.pullRequestId) else { return true }
                return !((pr.mergeState == "conflict" || pr.mergeState == "failing") && sentBackPrs.contains(pr.id))
            }
            let landedEvents = events.filter { $0.kind == "landed" }
            let landed: [QueueRow] = landedEvents.prefix(Limits.history).compactMap { e in
                guard let pr = prOf(e.pullRequestId) else { return nil }
                let train = present(e.mergeTrainId).flatMap { id in input.trains.first { $0.id == id } }
                let sha = present(e.sha).map { String($0.prefix(7)) }
                let others = landedEvents
                    .filter { o in present(o.mergeTrainId) != nil && o.mergeTrainId == e.mergeTrainId && o.pullRequestId != e.pullRequestId }
                    .compactMap { prOf($0.pullRequestId)?.number }
                    .sorted()
                let untested = train?.untested == true
                var parts = untested ? [copy.mergedNow] : others.isEmpty ? [] : [copy.landedWith.replacingOccurrences(of: "{numbers}", with: numbers(others))]
                if let sha { parts.append(sha) }
                return QueueRow(
                    id: e.id, pullRequestId: pr.id, identifier: "#\(pr.number)", title: pr.title,
                    subtitle: parts.isEmpty ? nil : parts.joined(separator: " · "),
                    glyph: untested ? "zap" : "ci-passed", tone: untested ? "warning" : "success", at: e.createdAt
                )
            }
            let isToday = { (e: TrainEvent) in localDay(msOf(e.createdAt) ?? 0, input.utcOffsetMinutes) == today }
            let base = testing.first.flatMap { now in input.trains.first { $0.id == now.trainId }?.base }
                ?? open.first { $0.mergeState == "queued" }?.base ?? events.first?.base ?? open.first?.base ?? "main"
            out.append(QueueRepo(
                repositoryId: repositoryId,
                title: repoName(input.repos, repositoryId),
                caption: copy.into.replacingOccurrences(of: "{base}", with: base),
                stats: QueueStats(
                    inQueue: testing.reduce(0) { $0 + $1.members.count } + upNext.count,
                    landedToday: landedEvents.filter(isToday).count,
                    sentBackToday: events.filter { ($0.kind == "failing" || $0.kind == "conflict") && isToday($0) }.count,
                    typicalRun: typical.map { PrChecks.formatDuration(ms: $0) }
                ),
                testing: testing,
                upNext: upNext,
                waiting: notReady,
                sentBack: sentBack,
                landed: landed
            ))
        }
        return out.sorted { a, b in a.title != b.title ? jsLess(a.title, b.title) : jsLess(a.repositoryId, b.repositoryId) }
    }

    // MARK: - The run page's context

    public struct RunOutcome: Equatable, Sendable {
        public let pullRequestId: String
        public let identifier: String
        public let title: String
        public let text: String
        public let glyph: String
        public let tone: String
        /// A failing PR's red job IN THIS RUN (Open log selects it).
        public let jobId: String?

        public init(pullRequestId: String, identifier: String, title: String, text: String, glyph: String, tone: String, jobId: String?) {
            self.pullRequestId = pullRequestId
            self.identifier = identifier
            self.title = title
            self.text = text
            self.glyph = glyph
            self.tone = tone
            self.jobId = jobId
        }
    }

    public struct RunContext: Equatable, Sendable {
        public let kind: RunKind
        /// copy.kinds[kind]; a pull request run `Pull request #n`.
        public let badge: String
        /// A merge queue run's batch title, else nil.
        public let title: String?
        /// A merge queue run: `main @ 3db841e`.
        public let caption: String?
        /// A merge queue run: what happened to each PR of its train.
        public let outcome: [RunOutcome]

        public init(kind: RunKind, badge: String, title: String?, caption: String?, outcome: [RunOutcome]) {
            self.kind = kind
            self.badge = badge
            self.title = title
            self.caption = caption
            self.outcome = outcome
        }
    }

    /// `runContextCases`: what the run page says about WHY the run exists
    /// (the TS `ciRunContext` doc comment is the rule). Reads repos,
    /// pullRequests, trains, trainEvents and jobs of `input`.
    public static func runContext(_ run: Run, _ input: Input) -> RunContext {
        let kind = runKind(run, repos: input.repos)
        let pr = run.pullRequestId.flatMap { id in input.pullRequests.first { $0.id == id } }
        let badge = kind == .pullRequest && pr != nil
            ? Copy.pullRequest.replacingOccurrences(of: "{number}", with: String(pr!.number))
            : Copy.kinds[kind.rawValue] ?? kind.rawValue
        let train = present(run.mergeTrainId).flatMap { id in input.trains.first { $0.id == id } }
        guard kind == .train, let train else {
            return RunContext(
                kind: kind, badge: badge,
                title: kind == .train ? batchTitle(run, pullRequests: input.pullRequests, trains: input.trains) : nil,
                caption: nil, outcome: []
            )
        }
        let out = Copy.Queue.outcome
        let outcome: [RunOutcome] = train.units.flatMap { $0 }.compactMap { id in
            guard let member = input.pullRequests.first(where: { $0.id == id }) else { return nil }
            let event = stableSorted(input.trainEvents.filter {
                $0.pullRequestId == id && $0.mergeTrainId == train.id && ["landed", "failing", "conflict"].contains($0.kind)
            }) { a, b in
                let (x, y) = (msOf(a.createdAt) ?? 0, msOf(b.createdAt) ?? 0)
                return x == y ? 0 : y < x ? -1 : 1
            }.first
            func row(_ text: String, _ glyph: String, _ tone: String, _ jobId: String? = nil) -> RunOutcome {
                RunOutcome(pullRequestId: id, identifier: "#\(member.number)", title: member.title, text: text, glyph: glyph, tone: tone, jobId: jobId)
            }
            switch event?.kind {
            case "landed":
                return train.untested
                    ? row(out["untested"]!, "zap", "warning")
                    : row(out["landed"]!.replacingOccurrences(of: "{sha}", with: String((event!.sha ?? "").prefix(7))), "ci-passed", "success")
            case "failing":
                let e = event!
                let job = present(e.ciJobId).flatMap { jobId in input.jobs.first { $0.id == jobId && $0.runId == run.id } }
                return row(
                    out["failing"]!.replacingOccurrences(of: "{checks}", with: listOf(e.checks, 2)).replacingOccurrences(of: "{tries}", with: tries(e.attempt)),
                    "ci-failed", "danger", job?.id
                )
            case "conflict":
                let e = event!
                return row(
                    out["conflict"]!.replacingOccurrences(of: "{files}", with: listOf(e.files.map(fileName), 2)).replacingOccurrences(of: "{tries}", with: tries(e.attempt)),
                    "pr-conflict", "danger"
                )
            default:
                if member.mergeState == "testing" && member.mergeTrainId == train.id { return row(out["testing"]!, "ci-running", "info") }
                return row(out["requeued"]!, "ci-queued", "neutral")
            }
        }
        return RunContext(
            kind: kind, badge: badge,
            title: batchTitle(run, pullRequests: input.pullRequests, trains: input.trains),
            caption: baseLine(train),
            outcome: outcome
        )
    }
}
