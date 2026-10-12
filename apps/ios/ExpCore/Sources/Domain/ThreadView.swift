import Foundation

// EXP-1284: what a THREAD looks like, resolved once and rendered identically
// ×4 (web `lib/thread-view.ts` = the spec, desktop `domain/src/thread_view.rs`,
// Android `domain/ThreadView.kt`), locked by
// `packages/domain-contract/fixtures/thread-view.json` (`ThreadViewTests`).
// A thread holds n issues and m runs (`thread_items`); this turns the synced
// rows into:
//
//   * its TITLE (the stored one, else the first run's name unless that is
//     the generic `Chat`, else the first issue's title, else `Chat`/`Thread`);
//   * its CHILD STRIP: Conversation, one chip per issue in link order, the
//     "+" and the Guide (dot when the thread's Guide has entries). On phones
//     the strip IS the pager;
//   * its CAPTIONS: the header's `2 runs · 2 devices` and a list row's
//     `2 issues · 1 run · macbook` (the device part only while a run is
//     live, naming the live runs' devices);
//   * its CONVERSATION: every run in start order as a `Run N` block (state,
//     device, `resumed`, `started by Session K`), the issues its agent linked as
//     rows under it (`Linked` / `Filed and linked`), and the runs one parent
//     started as ONE parallel card once there are two or more of them;
//   * its LIST placement: Running (a live run of mine) vs Recent.
//
// Pure: no queries, no SwiftUI. Callers pass what they synced; an item whose
// issue or run did not resolve is dropped (trashed board, purged run).

/// The thread row the view reads. `results` = the thread's Guide as the raw
/// jsonb TEXT (`ThreadEntity.results`); only its being a non-empty array
/// matters here.
public struct ThreadViewThread: Equatable, Sendable {
    public let id: String
    public let title: String?
    public let results: String?
    public let updatedAt: String

    public init(id: String, title: String?, results: String?, updatedAt: String) {
        self.id = id
        self.title = title
        self.results = results
        self.updatedAt = updatedAt
    }

    public init(_ entity: ThreadEntity) {
        self.init(id: entity.id, title: entity.title, results: entity.results, updatedAt: entity.updatedAt)
    }
}

public struct ThreadViewItem: Equatable, Sendable {
    public let id: String
    /// `issue` | `run` (contract `threadItemKind`).
    public let kind: String
    public let issueId: String?
    public let sessionId: String?
    /// `user` | `agent` (contract `threadItemSource`).
    public let source: String
    public let addedBySessionId: String?
    public let sortOrder: Double
    public let createdAt: String

    public init(
        id: String,
        kind: String,
        issueId: String?,
        sessionId: String?,
        source: String,
        addedBySessionId: String?,
        sortOrder: Double,
        createdAt: String
    ) {
        self.id = id
        self.kind = kind
        self.issueId = issueId
        self.sessionId = sessionId
        self.source = source
        self.addedBySessionId = addedBySessionId
        self.sortOrder = sortOrder
        self.createdAt = createdAt
    }

    public init(_ entity: ThreadItemEntity) {
        self.init(
            id: entity.id,
            kind: entity.kind,
            issueId: entity.issueId,
            sessionId: entity.sessionId,
            source: entity.source,
            addedBySessionId: entity.addedBySessionId,
            sortOrder: entity.sortOrder,
            createdAt: entity.createdAt
        )
    }
}

public struct ThreadViewIssue: Equatable, Sendable {
    public let id: String
    public let identifier: String
    public let title: String
    public let createdAt: String

    public init(id: String, identifier: String, title: String, createdAt: String) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.createdAt = createdAt
    }

    public init(_ entity: IssueEntity) {
        self.init(
            id: entity.id, identifier: entity.identifier ?? "", title: entity.title, createdAt: entity.createdAt
        )
    }
}

public struct ThreadViewRun: Equatable, Sendable {
    public let id: String
    /// The run's identity subject (the session row title: issue title,
    /// action name, chat title…), used where a run is named on its own.
    public let name: String
    public let userId: String
    public let deviceLabel: String?
    /// `running` | `in_review` | `ended`.
    public let status: String
    public let needsInput: Bool
    public let agentBusy: Bool
    public let issueId: String?
    public let parentSessionId: String?
    public let resumedFromId: String?
    public let startedAt: String
    public let endedAt: String?

    public init(
        id: String,
        name: String,
        userId: String,
        deviceLabel: String?,
        status: String,
        needsInput: Bool,
        agentBusy: Bool,
        issueId: String?,
        parentSessionId: String?,
        resumedFromId: String?,
        startedAt: String,
        endedAt: String?
    ) {
        self.id = id
        self.name = name
        self.userId = userId
        self.deviceLabel = deviceLabel
        self.status = status
        self.needsInput = needsInput
        self.agentBusy = agentBusy
        self.issueId = issueId
        self.parentSessionId = parentSessionId
        self.resumedFromId = resumedFromId
        self.startedAt = startedAt
        self.endedAt = endedAt
    }

    /// A synced run under its display `name` (the caller's session subject,
    /// `sessionRowTitle`) and the device label it presents with.
    public init(_ session: CodingSessionEntity, name: String, deviceLabel: String?) {
        self.init(
            id: session.id,
            name: name,
            userId: session.userId,
            deviceLabel: deviceLabel,
            status: session.status,
            needsInput: session.needsInput,
            agentBusy: session.agentBusy,
            issueId: session.issueId,
            parentSessionId: session.parentSessionId,
            resumedFromId: session.resumedFromId,
            startedAt: session.startedAt,
            endedAt: session.endedAt
        )
    }
}

public enum ThreadRunState: String, Equatable, Sendable {
    case live
    case needsInput = "needs_input"
    case ended
}

public enum ThreadChild: Equatable, Hashable, Sendable {
    case conversation
    case issue(issueId: String, identifier: String, title: String)
    case add
    case guide(dot: Bool)
}

public struct ThreadLinkRow: Equatable, Sendable {
    public let itemId: String
    public let issueId: String
    public let identifier: String
    public let title: String
    /// `Linked` | `Filed and linked` (the agent filed it, then linked it).
    public let label: String

    public init(itemId: String, issueId: String, identifier: String, title: String, label: String) {
        self.itemId = itemId
        self.issueId = issueId
        self.identifier = identifier
        self.title = title
        self.label = label
    }
}

public struct ThreadRunBlock: Equatable, Sendable {
    public let runId: String
    /// `Run 1`, … in start order across the whole thread.
    public let label: String
    /// The muted parts after the label: device, `resumed`, `started by Session K`.
    public let caption: [String]
    public let state: ThreadRunState
    /// Ended runs fold to one line until Show work opens them.
    public let folded: Bool
    public let links: [ThreadLinkRow]

    public init(
        runId: String,
        label: String,
        caption: [String],
        state: ThreadRunState,
        folded: Bool,
        links: [ThreadLinkRow]
    ) {
        self.runId = runId
        self.label = label
        self.caption = caption
        self.state = state
        self.folded = folded
        self.links = links
    }
}

public struct ThreadParallelRow: Equatable, Sendable {
    public let runId: String
    public let name: String
    public let issueId: String?
    public let deviceLabel: String?
    public let state: ThreadRunState
    public let startedAt: String

    public init(
        runId: String,
        name: String,
        issueId: String?,
        deviceLabel: String?,
        state: ThreadRunState,
        startedAt: String
    ) {
        self.runId = runId
        self.name = name
        self.issueId = issueId
        self.deviceLabel = deviceLabel
        self.state = state
        self.startedAt = startedAt
    }
}

public struct ThreadParallelBlock: Equatable, Sendable {
    /// `Runs 3 and 4`, `Runs 3, 4 and 5`.
    public let label: String
    /// `started by Session 2`.
    public let caption: [String]
    /// Live while any of its runs is.
    public let state: ThreadRunState
    public let runs: [ThreadParallelRow]
    public let links: [ThreadLinkRow]

    public init(
        label: String,
        caption: [String],
        state: ThreadRunState,
        runs: [ThreadParallelRow],
        links: [ThreadLinkRow]
    ) {
        self.label = label
        self.caption = caption
        self.state = state
        self.runs = runs
        self.links = links
    }
}

public enum ThreadBlock: Equatable, Sendable {
    case run(ThreadRunBlock)
    case parallel(ThreadParallelBlock)
}

public struct ThreadViewModel: Equatable, Sendable {
    public let title: String
    /// True when `title` is derived (the thread has no stored title).
    public let titleDerived: Bool
    public let strip: [ThreadChild]
    public let headerCaption: String
    public let listCaption: String
    public let live: Bool
    public let needsInput: Bool
    public let busy: Bool
    public let conversation: [ThreadBlock]
    /// Run ids in start order (Run 1 first).
    public let runIds: [String]
    /// Issue ids in link order.
    public let issueIds: [String]

    public init(
        title: String,
        titleDerived: Bool,
        strip: [ThreadChild],
        headerCaption: String,
        listCaption: String,
        live: Bool,
        needsInput: Bool,
        busy: Bool,
        conversation: [ThreadBlock],
        runIds: [String],
        issueIds: [String]
    ) {
        self.title = title
        self.titleDerived = titleDerived
        self.strip = strip
        self.headerCaption = headerCaption
        self.listCaption = listCaption
        self.live = live
        self.needsInput = needsInput
        self.busy = busy
        self.conversation = conversation
        self.runIds = runIds
        self.issueIds = issueIds
    }
}

/// One thread in a list (`partitionThreadList`).
public protocol ThreadListEntry {
    var threadId: String { get }
    var teamId: String { get }
    var updatedAt: String { get }
    /// The viewer owns a live session in it (heartbeat-live).
    var mineLive: Bool { get }
    /// The viewer has a session in it or created it (Recent lists only these).
    var mine: Bool { get }
}

/// One PR a thread's Guide could target (`threadPrTarget`): the PRs on the
/// thread's sessions and issues.
public struct ThreadPrCandidate: Equatable, Sendable {
    public let url: String
    /// `open` | `merged` | `closed` (any other value counts as closed).
    public let state: String?
    /// When the PR was stamped (its session's or issue's activity time).
    public let at: String

    public init(url: String, state: String?, at: String) {
        self.url = url
        self.state = state
        self.at = at
    }
}

public enum ThreadViews {
    public static let fallbackTitle = "Thread"
    public static let linkedLabel = "Linked"
    public static let filedLabel = "Filed and linked"
    /// An issue created at most this long before the agent linked it was
    /// FILED by that agent for the thread.
    public static let filedWindowMs: Double = 120_000
    /// The generic chat run name (web `BUILTIN_CHAT_NAME`).
    public static let chatName = PastRuns.chatRunName

    // MARK: - Copy (EXP-1284, byte-identical ×4)

    public static let conversationLabel = "Conversation"
    public static let guideLabel = "Guide"
    /// The muted word beside the title.
    public static let threadWord = "thread"
    public static let linkIssueLabel = "Link an issue"
    public static let newIssueLabel = "New issue in this thread"
    public static let startRunLabel = "Start a session here"
    public static let replyPlaceholder = "Reply in this thread"
    public static let threadsLabel = "Threads"
    /// The toolbar history sheet's title (its rows = my recent threads).
    public static let recentLabel = "Recent"
    public static let showWorkLabel = "Show work"

    /// A "New issue in this thread" draft's line: `In <thread title>`.
    public static let inThreadPrefix = "In"

    public static func inThreadLabel(_ title: String) -> String {
        "\(inThreadPrefix) \(title)"
    }

    public static func unlinkLabel(_ identifier: String) -> String {
        "Unlink \(identifier)"
    }

    /// The trailing state word of a block header.
    public static func stateWord(_ state: ThreadRunState) -> String {
        switch state {
        case .live: "live"
        case .needsInput: "needs input"
        case .ended: "ended"
        }
    }

    public static func runState(status: String, needsInput: Bool) -> ThreadRunState {
        if status == DomainContract.codingSessionStatusEnded { return .ended }
        return needsInput ? .needsInput : .live
    }

    public static func runState(_ run: ThreadViewRun) -> ThreadRunState {
        runState(status: run.status, needsInput: run.needsInput)
    }

    // MARK: - Helpers (web parity)

    /// `Date.parse` with a 0 for anything unparsable.
    private static func time(_ value: String?) -> Double {
        guard let value, let date = WireTimestamps.parse(value) else { return 0 }
        return date.timeIntervalSince1970 * 1000
    }

    private static func plural(_ count: Int, _ one: String, _ many: String) -> String {
        "\(count) \(count == 1 ? one : many)"
    }

    /// `3`, `3 and 4`, `3, 4 and 5`.
    private static func joinOrdinals(_ ordinals: [Int]) -> String {
        guard ordinals.count > 1 else { return ordinals.map(String.init).joined() }
        let head = ordinals.dropLast().map(String.init).joined(separator: ", ")
        return "\(head) and \(ordinals[ordinals.count - 1])"
    }

    private static func trimmed(_ value: String?) -> String {
        (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// One device by name, `k devices` for several, nil for none (unique
    /// labels in first-seen order).
    private static func devicesPart(_ runs: [ThreadViewRun]) -> String? {
        var labels: [String] = []
        for run in runs {
            let label = trimmed(run.deviceLabel)
            if !label.isEmpty, !labels.contains(label) { labels.append(label) }
        }
        if labels.isEmpty { return nil }
        return labels.count == 1 ? labels[0] : "\(labels.count) devices"
    }

    /// Whether the thread's Guide (raw jsonb text) is a non-empty array.
    public static func hasGuide(_ results: String?) -> Bool {
        guard let results, let data = results.data(using: .utf8),
              let array = (try? JSONSerialization.jsonObject(with: data)) as? [Any]
        else { return false }
        return !array.isEmpty
    }

    // MARK: - buildThreadView

    public static func build(
        thread: ThreadViewThread,
        items rawItems: [ThreadViewItem],
        issues rawIssues: [ThreadViewIssue],
        runs rawRuns: [ThreadViewRun]
    ) -> ThreadViewModel {
        let issuesById = Dictionary(rawIssues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let runsById = Dictionary(rawRuns.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let items = rawItems.sorted { a, b in
            if a.sortOrder != b.sortOrder { return a.sortOrder < b.sortOrder }
            let ta = time(a.createdAt), tb = time(b.createdAt)
            if ta != tb { return ta < tb }
            return a.id < b.id
        }

        let issueItems = items.filter { item in
            item.kind == DomainContract.threadItemKindIssue
                && item.issueId.map { issuesById[$0] != nil } == true
        }
        let issues = issueItems.compactMap { $0.issueId.flatMap { issuesById[$0] } }
        let runs = items
            .filter { item in
                item.kind == DomainContract.threadItemKindRun
                    && item.sessionId.map { runsById[$0] != nil } == true
            }
            .compactMap { $0.sessionId.flatMap { runsById[$0] } }
            .sorted { a, b in
                let ta = time(a.startedAt), tb = time(b.startedAt)
                if ta != tb { return ta < tb }
                return a.id < b.id
            }

        // The thread is named after its first prompt (the first run's own
        // name: a chat's agent title, an action's name, the issue a run
        // started on), the first issue only while that run is still the
        // generic `Chat`.
        let stored = trimmed(thread.title)
        let firstRun = trimmed(runs.first?.name)
        let firstIssue = trimmed(issues.first?.title)
        let derived: String
        if !firstRun.isEmpty, firstRun != chatName {
            derived = firstRun
        } else if !firstIssue.isEmpty {
            derived = firstIssue
        } else if !firstRun.isEmpty {
            derived = firstRun
        } else {
            derived = fallbackTitle
        }
        let title = stored.isEmpty ? derived : stored

        var strip: [ThreadChild] = [.conversation]
        strip += issues.map { .issue(issueId: $0.id, identifier: $0.identifier, title: $0.title) }
        strip.append(.add)
        strip.append(.guide(dot: hasGuide(thread.results)))

        let liveRuns = runs.filter { $0.status != DomainContract.codingSessionStatusEnded }
        let runsPart = plural(runs.count, "session", "sessions")
        let headerParts = [runsPart, devicesPart(runs)].compactMap { $0 }
        let issuesPart = issues.isEmpty ? "no issues" : plural(issues.count, "issue", "issues")
        let listParts = [issuesPart, runsPart, liveRuns.isEmpty ? nil : devicesPart(liveRuns)]
            .compactMap { $0 }

        var ordinal: [String: Int] = [:]
        for (index, run) in runs.enumerated() { ordinal[run.id] = index + 1 }

        func linksOf(_ runIds: Set<String>) -> [ThreadLinkRow] {
            issueItems
                .filter { item in
                    item.source == DomainContract.threadItemSourceAgent
                        && item.addedBySessionId.map { runIds.contains($0) } == true
                }
                .sorted { a, b in
                    let ta = time(a.createdAt), tb = time(b.createdAt)
                    if ta != tb { return ta < tb }
                    return a.id < b.id
                }
                .compactMap { item in
                    guard let issue = item.issueId.flatMap({ issuesById[$0] }) else { return nil }
                    let filed = time(issue.createdAt) >= time(item.createdAt) - filedWindowMs
                    return ThreadLinkRow(
                        itemId: item.id,
                        issueId: issue.id,
                        identifier: issue.identifier,
                        title: issue.title,
                        label: filed ? filedLabel : linkedLabel
                    )
                }
        }

        // Children one in-thread parent started, grouped once there are 2+.
        var childrenOf: [String: [ThreadViewRun]] = [:]
        for run in runs {
            if let parent = run.parentSessionId, ordinal[parent] != nil, run.resumedFromId == nil {
                childrenOf[parent, default: []].append(run)
            }
        }

        var conversation: [ThreadBlock] = []
        var placed = Set<String>()
        for run in runs {
            if placed.contains(run.id) { continue }
            let siblings: [ThreadViewRun]? = {
                guard let parent = run.parentSessionId, run.resumedFromId == nil else { return nil }
                return childrenOf[parent]
            }()
            let startedBy: String? = run.parentSessionId
                .flatMap { ordinal[$0] }
                .map { "started by Session \($0)" }
            if let siblings, siblings.count >= 2 {
                for sibling in siblings { placed.insert(sibling.id) }
                let states = siblings.map { runState($0) }
                let state: ThreadRunState = states.contains(.needsInput)
                    ? .needsInput
                    : (states.contains(.live) ? .live : .ended)
                conversation.append(.parallel(ThreadParallelBlock(
                    label: "Sessions \(joinOrdinals(siblings.compactMap { ordinal[$0.id] }))",
                    caption: startedBy.map { [$0] } ?? [],
                    state: state,
                    runs: siblings.map { sibling in
                        ThreadParallelRow(
                            runId: sibling.id,
                            name: sibling.name,
                            issueId: sibling.issueId,
                            deviceLabel: sibling.deviceLabel,
                            state: runState(sibling),
                            startedAt: sibling.startedAt
                        )
                    },
                    links: linksOf(Set(siblings.map(\.id)))
                )))
                continue
            }
            placed.insert(run.id)
            let state = runState(run)
            let device = trimmed(run.deviceLabel)
            let caption = [
                device.isEmpty ? nil : device,
                run.resumedFromId != nil ? "resumed" : nil,
                startedBy,
            ].compactMap { $0 }
            conversation.append(.run(ThreadRunBlock(
                runId: run.id,
                label: "Session \(ordinal[run.id] ?? 0)",
                caption: caption,
                state: state,
                folded: state == .ended,
                links: linksOf([run.id])
            )))
        }

        return ThreadViewModel(
            title: title,
            titleDerived: stored.isEmpty,
            strip: strip,
            headerCaption: headerParts.joined(separator: " · "),
            listCaption: listParts.joined(separator: " · "),
            live: !liveRuns.isEmpty,
            needsInput: liveRuns.contains { $0.needsInput },
            busy: liveRuns.contains { $0.agentBusy },
            conversation: conversation,
            runIds: runs.map(\.id),
            issueIds: issues.map(\.id)
        )
    }

    // MARK: - partitionThreadList

    /// Running (a live session of mine) vs Recent (my other threads: a
    /// session of mine or created by me), newest activity first. `teamId` =
    /// the ACTIVE team (web, desktop); nil = every team (phones, which band
    /// the rows per team).
    public static func partition<T: ThreadListEntry>(
        _ entries: [T], teamId: String?
    ) -> (running: [T], recent: [T]) {
        let byActivity = entries
            .filter { teamId == nil || $0.teamId == teamId }
            .sorted { a, b in
                let ta = time(a.updatedAt), tb = time(b.updatedAt)
                if ta != tb { return ta > tb }
                return a.threadId < b.threadId
            }
        return (
            running: byActivity.filter(\.mineLive),
            recent: byActivity.filter { !$0.mineLive && $0.mine }
        )
    }

    // MARK: - threadPrTarget

    /// The PR a thread's Guide and Merge capsule target: the newest OPEN PR,
    /// else the newest MERGED one, else none.
    public static func prTarget(_ candidates: [ThreadPrCandidate]) -> String? {
        func newest(_ state: String) -> String? {
            candidates
                .filter { $0.state == state && !$0.url.isEmpty }
                .sorted { a, b in
                    let ta = time(a.at), tb = time(b.at)
                    if ta != tb { return ta > tb }
                    return a.url < b.url
                }
                .first?.url
        }
        return newest(DomainContract.prStateOpen) ?? newest(DomainContract.prStateMerged)
    }

    // MARK: - sessionGuideEntries

    /// The Guide entries a session block shows: the ones that session wrote
    /// (`sessionId`); entries filed before the stamp belong to the thread's
    /// FIRST session. The thread's Guide face shows every entry. Raw jsonb
    /// text in (`thread.results`) and out; nil when `raw` is nil or does not
    /// parse as an array (= no entries).
    public static func sessionGuideEntries(
        _ raw: String?, sessionId: String, firstSessionId: String?
    ) -> String? {
        guard let raw, let data = raw.data(using: .utf8),
              let array = (try? JSONSerialization.jsonObject(with: data)) as? [Any]
        else { return nil }
        let kept = array.filter { entry in
            let stamped = ((entry as? [String: Any])?["sessionId"] as? String) ?? ""
            return stamped.isEmpty ? sessionId == firstSessionId : stamped == sessionId
        }
        guard let out = try? JSONSerialization.data(withJSONObject: kept),
              let text = String(data: out, encoding: .utf8)
        else { return nil }
        return text
    }
}

// MARK: - iOS-local rules over the model (EXP-1284)

public extension ThreadViews {
    /// The Guide a run's readers show: its THREAD's Guide when the run sits
    /// in a thread (the canonical one), else the run's own row — web
    /// `guideOfRun` (`threadResults ?? results`).
    static func guideResults(threadResults: String?, runResults: String?) -> String? {
        threadResults ?? runResults
    }

    /// The THREAD page's Guide (T-N8): the thread's own, else its NEWEST
    /// session's row (a session just moved into a fresh thread has written
    /// none there yet). `sessions` in start order.
    static func threadGuide(threadResults: String?, sessions: [CodingSessionEntity]) -> String? {
        threadResults ?? sessions.last { $0.results != nil }?.results
    }

    /// What the thread's reply box does (brief §2): send into the latest LIVE
    /// run the viewer owns; else, when the latest run is the viewer's (and
    /// ended), RESUME it; else start a new run in the thread.
    enum ReplyTarget: Equatable, Sendable {
        case send(runId: String)
        case resume(runId: String)
        case start
    }

    /// `runs` in start order (Run 1 first).
    static func replyTarget(runs: [ThreadViewRun], me: String?) -> ReplyTarget {
        guard let me else { return .start }
        if let live = runs.last(where: {
            $0.userId == me && $0.status != DomainContract.codingSessionStatusEnded
        }) {
            return .send(runId: live.id)
        }
        if let latest = runs.last, latest.userId == me {
            return .resume(runId: latest.id)
        }
        return .start
    }

    /// The thread's PR (`threadPrTarget` over the PRs on its sessions and
    /// issues, `at` = their `updated_at`) resolved to what the Guide reads:
    /// its diff source (the issue carrying it, else the session) and, only
    /// while it is OPEN, the Merge capsule's target.
    struct PrTarget: Equatable, Sendable {
        public let url: String
        public let open: Bool
        /// The diff / merge subject: an issue with that PR, else a session.
        public let subject: MergeTarget

        public init(url: String, open: Bool, subject: MergeTarget) {
            self.url = url
            self.open = open
            self.subject = subject
        }

        /// The Merge capsule's target — an open PR only.
        public var mergeTarget: MergeTarget? { open ? subject : nil }
    }

    /// `sessions` = the thread's sessions; `issues` = its linked issues (the
    /// rows carrying their PRs; a batch's covered ones too).
    static func prTarget(
        sessions: [CodingSessionEntity],
        issues: [IssueEntity]
    ) -> PrTarget? {
        var candidates: [ThreadPrCandidate] = []
        for issue in issues {
            if let url = issue.prUrl, !url.isEmpty {
                candidates.append(ThreadPrCandidate(url: url, state: issue.prState, at: issue.updatedAt))
            }
        }
        for session in sessions {
            if let url = session.prUrl, !url.isEmpty {
                candidates.append(ThreadPrCandidate(url: url, state: session.prState, at: session.updatedAt))
            }
        }
        guard let url = prTarget(candidates) else { return nil }
        let open = candidates.contains { $0.url == url && $0.state == DomainContract.prStateOpen }
        if let issue = issues.first(where: { $0.prUrl == url }) {
            return PrTarget(url: url, open: open, subject: .issue(issueId: issue.id))
        }
        if let session = sessions.last(where: { $0.prUrl == url }) {
            return PrTarget(url: url, open: open, subject: .session(sessionId: session.id))
        }
        return nil
    }

    /// Whether a strip child was linked moments ago, so it fades in with a
    /// ring the first time it renders (brief §1): created < 10 s before now.
    static let freshChildWindow: TimeInterval = 10

    static func isFresh(createdAt: String, now: Date) -> Bool {
        guard let created = WireTimestamps.parse(createdAt) else { return false }
        let age = now.timeIntervalSince(created)
        return age >= 0 && age < freshChildWindow
    }
}
