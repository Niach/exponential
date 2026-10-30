import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-1291: the session tree as the phone's LISTS read it — ONE observation
/// of the synced `sessions` / `session_issues` / `conversations` (their
/// digests, never `messages`) / `pull_requests` rows, owned by
/// the navigator for its whole lifetime and handed down through the
/// environment, so the Threads tab, its dot, Home's Running band and the
/// board switcher's per-team dots never disagree. Every rule is ExpCore's
/// `SessionTree` (fixture-locked ×4); this model only maps rows and keeps
/// the derived lists.
@MainActor @Observable
final class SessionListsModel {
    /// Every team's thread groups (`partitionThreads(teamId: nil)` — phones
    /// band the rows per team).
    private(set) var groups: [ThreadsListGroup] = []
    /// Every team's agents (`listAgents(teamId: nil)`, NEW-2: the synced
    /// `agents` definitions).
    private(set) var agents: [AgentRow] = []
    /// NEW-2: my conversation session with each agent (a root with
    /// `agent_id` I started), by agent id; absent = none yet.
    private(set) var agentConversations: [String: String] = [:]
    /// Home's Running band: my live threads — needs-you first, then running,
    /// each newest activity first.
    private(set) var running: [ThreadsListRow] = []
    /// A thread's first linked issue identifier (the row's mono id).
    private(set) var identifiers: [String: String] = [:]
    /// A session's agent CLI (the row's run mark), by session id (EXP-1323:
    /// nested child rows too).
    private(set) var clis: [String: String] = [:]
    /// EXP-1309: a Threads row's quiet word off its pull requests, by root id
    /// (`SessionTree.threadRowWord`).
    private(set) var words: [String: ThreadRowWord] = [:]
    /// NEW-41: each root's pull-request states (the Threads tab's lead glyph).
    private(set) var prStates: [String: [String]] = [:]
    /// EXP-1316: a thread row's device glyph (the ROOT's `device_id` against
    /// the synced devices rows), by root id; absent = no glyph.
    private(set) var devices: [String: ThreadRowDevice] = [:]
    /// The board switcher's per-team dots (`liveByTeam`).
    private(set) var liveByTeam: [String: SessionTree.TeamLiveSessions] = [:]
    /// The first emission landed (before it, lists draw skeleton rows).
    private(set) var loaded = false
    /// EXP-1373 (chats.json): the list inputs — every conversation's digest
    /// back as messages (`Chats.digestMessages`) — `Chats.chatList`'s
    /// `messages`, never a whole transcript.
    private(set) var chatMessages: [TreeMessage] = []
    private(set) var reads: [TreeSessionRead] = []
    /// Every session (the agent bubbles' conversations).
    private(set) var sessions: [TreeSession] = []
    /// Each root's status (the chat menu: live vs ended) and starter.
    private(set) var rootStatus: [String: String] = [:]
    private(set) var rootStarter: [String: String] = [:]

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var boundAccountId: String?

    nonisolated init() {}

    /// The Threads tab's green dot: a thread runs.
    var threadsRunning: Bool {
        groups.contains { $0.key == .running }
    }

    /// EXP-1323: a thread row is unread (`threadsUnread`) — the Threads
    /// tab's dot when nothing runs or waits.
    var threadsUnread: Bool {
        SessionTree.threadsUnread(groups)
    }

    /// The Threads tab's amber dot: a thread waits on a person (the
    /// Needs-you group). Agents have their own rail rows, so they never
    /// light it (web/Android parity, EXP-1327).
    var needsYou: Bool {
        groups.contains { $0.key == .needsYou }
    }

    /// EXP-1373: ONE active team drives the phone: the Chats dot reads its
    /// rows alone.
    func chatsRunning(teamId: String?) -> Bool {
        groups.contains { $0.key == .running && $0.rows.contains { teamId == nil || $0.teamId == teamId } }
    }

    func chatsNeedYou(teamId: String?) -> Bool {
        groups.contains { $0.key == .needsYou && $0.rows.contains { teamId == nil || $0.teamId == teamId } }
    }

    /// #EXP-1388 (`dotCases`): an archived chat never lights the dot.
    func chatsUnread(teamId: String?) -> Bool {
        Chats.chatsUnread(groups: teamGroups(teamId), reads: reads)
    }

    /// `partitionThreads` narrowed to one team (empty groups dropped).
    func teamGroups(_ teamId: String?) -> [ThreadsListGroup] {
        guard let teamId else { return groups }
        return groups.compactMap { group in
            let rows = group.rows.filter { $0.teamId == teamId }
            return rows.isEmpty ? nil : ThreadsListGroup(key: group.key, title: group.title, rows: rows)
        }
    }

    /// (Re)binds to an account's pool; a no-op while already bound to it.
    func bind(accountId: String, userId: String?, db: DatabaseManager) {
        guard boundAccountId != accountId || task == nil else { return }
        stop()
        boundAccountId = accountId
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        // EXP-1358: `derive` runs inside the observation's `map` — on GRDB's
        // reader, never the main actor (this model is `@MainActor`, so the
        // loop below is) — and an emission equal to the last one is dropped
        // before it can re-render the Threads tab, the tab bar and Home.
        let observation = ValueObservation.tracking { db in
            try Self.read(db)
        }.map { rows in
            Self.derive(rows, userId: userId)
        }.removeDuplicates()
        task = Task { [weak self] in
            do {
                for try await derived in observation.values(in: pool) {
                    self?.apply(derived)
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        boundAccountId = nil
        groups = []
        agents = []
        running = []
        identifiers = [:]
        clis = [:]
        words = [:]
        prStates = [:]
        devices = [:]
        liveByTeam = [:]
        chatMessages = []
        reads = []
        sessions = []
        rootStatus = [:]
        rootStarter = [:]
        loaded = false
    }

    // MARK: - Rows

    private struct Rows: Sendable {
        var sessions: [TreeSession]
        var sessionIssues: [TreeSessionIssue]
        var issues: [TreeIssue]
        var messages: [TreeMessage]
        var reads: [TreeSessionRead]
        var clis: [String: String]
        var pullRequests: [PullRequestEntity]
        var deviceRows: [DeviceEntity]
        var agents: [AgentLike]
    }

    private struct Derived: Sendable, Equatable {
        var groups: [ThreadsListGroup]
        var agents: [AgentRow]
        var agentConversations: [String: String]
        var running: [ThreadsListRow]
        var identifiers: [String: String]
        var clis: [String: String]
        var words: [String: ThreadRowWord]
        var prStates: [String: [String]]
        var devices: [String: ThreadRowDevice]
        var liveByTeam: [String: SessionTree.TeamLiveSessions]
        var chatMessages: [TreeMessage]
        var reads: [TreeSessionRead]
        var sessions: [TreeSession]
        var rootStatus: [String: String]
        var rootStarter: [String: String]
    }

    private func apply(_ derived: Derived) {
        groups = derived.groups
        agents = derived.agents
        agentConversations = derived.agentConversations
        running = derived.running
        identifiers = derived.identifiers
        clis = derived.clis
        words = derived.words
        prStates = derived.prStates
        devices = derived.devices
        liveByTeam = derived.liveByTeam
        chatMessages = derived.chatMessages
        reads = derived.reads
        sessions = derived.sessions
        rootStatus = derived.rootStatus
        rootStarter = derived.rootStarter
        loaded = true
    }

    /// The whole session table (the shape already scopes it to what I may
    /// read), the root links, their issues, and every conversation's digest
    /// messages (the `SessionTitle` first-prompt rung, the previews, the
    /// waiting asks and the agent times — never a whole transcript).
    nonisolated private static func read(_ db: Database) throws -> Rows {
        let sessions = try SessionEntity.fetchAll(db)
        let links = try SessionIssueEntity.order(Column("sort_order")).fetchAll(db)
        let issueIds = Array(Set(links.map(\.issueId)))
        let issues = issueIds.isEmpty
            ? []
            : try IssueEntity.filter(issueIds.contains(Column("id"))).fetchAll(db)
        // Thread e4a04d22 (chats.json LIST DATA): the lists never read a
        // transcript — each conversation's server-kept digest, turned back
        // into messages (`Chats.digestMessages`, `digestCases`), stands in
        // for them, so this observation tracks `conversations`, never
        // `messages`.
        let rootIds = Dictionary(sessions.map { ($0.id, $0.rootId) }, uniquingKeysWith: { first, _ in first })
        let digestMessages = try Row.fetchAll(db, sql: """
            SELECT "session_id", "digest" FROM "conversations"
            WHERE "session_id" IS NOT NULL AND "session_id" <> ''
            """)
            .flatMap { row -> [TreeMessage] in
                let sessionId: String = row["session_id"]
                return Chats.digestMessages(sessionId: sessionId, digest: row["digest"] as String?, rootId: rootIds[sessionId])
            }
        return Rows(
            sessions: sessions.map { TreeSession(sessionRow: $0) },
            sessionIssues: links.map { TreeSessionIssue(sessionIssueRow: $0) },
            issues: issues.map { TreeIssue(issueRow: $0) },
            messages: digestMessages,
            reads: try SessionReadEntity.fetchAll(db).map { TreeSessionRead(sessionReadRow: $0) },
            clis: Dictionary(
                sessions.map { ($0.id, $0.cli ?? "") },
                uniquingKeysWith: { a, _ in a }
            ),
            pullRequests: try PullRequestEntity.fetchAll(db),
            deviceRows: try DeviceEntity.fetchAll(db),
            agents: try AgentDefinitionEntity.fetchAll(db).map(AgentLike.init)
        )
    }

    nonisolated private static func derive(_ rows: Rows, userId: String?) -> Derived {
        let groups = SessionTree.partitionThreads(
            sessions: rows.sessions,
            sessionIssues: rows.sessionIssues,
            issues: rows.issues,
            messages: rows.messages,
            teamId: nil,
            reads: rows.reads,
            // S-3: the clock the Recent window and the nested children's
            // `visibleChild` read (a phone draws roots only; a hidden child
            // folds into the `earlier` row, which opens the thread).
            now: ISO8601DateFormatter().string(from: Date())
        )
        // Roots holding a live session of mine (any depth of the tree).
        let mine = Set(rows.sessions.filter {
            $0.userId == userId && $0.status != DomainContract.sessionStatusEnded
        }.map(\.rootId))
        let running = groups
            .filter { $0.key != .recent }
            .flatMap(\.rows)
            .filter { $0.depth == 0 && mine.contains($0.sessionId) }
        let identifierById = Dictionary(
            rows.issues.map { ($0.id, $0.identifier) }, uniquingKeysWith: { a, _ in a }
        )
        var identifiers: [String: String] = [:]
        // `sessionIssues` arrive in link order: the first link per root wins.
        for link in rows.sessionIssues where identifiers[link.sessionId] == nil {
            if let identifier = identifierById[link.issueId], !identifier.isEmpty {
                identifiers[link.sessionId] = identifier
            }
        }
        // A stack edge may cross threads, never teams.
        let prsByTeam = Dictionary(grouping: rows.pullRequests, by: \.teamId)
        let prsByRoot = Dictionary(grouping: rows.pullRequests.filter { $0.rootSessionId != nil }) {
            $0.rootSessionId ?? ""
        }
        var words: [String: ThreadRowWord] = [:]
        var prStates: [String: [String]] = [:]
        // EXP-1323: a nested child row carries no word (web + IDE).
        for row in groups.flatMap(\.rows) where row.depth == 0 {
            let prs = prsByRoot[row.sessionId] ?? []
            words[row.sessionId] = SessionTree.threadRowWord(
                busy: row.busy,
                needsYou: row.needsYou,
                prs: prs,
                teamPrs: prsByTeam[row.teamId] ?? []
            )
            // NEW-41: the lead's PR glyph follows the root's PRs, not the word.
            prStates[row.sessionId] = prs.map(\.state)
        }
        // The root's device row (the starter's own first, like the session
        // view's host row); an unknown or invisible device draws nothing.
        let deviceRowsById = Dictionary(grouping: rows.deviceRows, by: \.deviceId)
        let starters = Dictionary(
            rows.sessions.map { ($0.id, $0.userId) }, uniquingKeysWith: { a, _ in a }
        )
        var devices: [String: ThreadRowDevice] = [:]
        for row in groups.flatMap(\.rows) {
            guard let deviceId = row.deviceId, let candidates = deviceRowsById[deviceId] else { continue }
            guard let device = candidates.first(where: { $0.userId == starters[row.sessionId] }) ?? candidates.first
            else { continue }
            devices[row.sessionId] = ThreadRowDevice(
                icon: DeviceIconDisplay.iconName(for: device),
                name: device.label
            )
        }
        return Derived(
            groups: groups,
            agents: SessionTree.listAgents(
                agents: rows.agents, sessions: rows.sessions, teamId: nil, me: userId
            ),
            agentConversations: Dictionary(
                rows.sessions.compactMap { session -> (String, String)? in
                    guard session.parentId == nil, session.userId == userId,
                          let agentId = session.agentId, !agentId.isEmpty
                    else { return nil }
                    return (agentId, session.id)
                },
                uniquingKeysWith: { a, _ in a }
            ),
            running: running,
            identifiers: identifiers,
            clis: rows.clis,
            words: words,
            prStates: prStates,
            devices: devices,
            liveByTeam: SessionTree.liveByTeam(rows.sessions, userId: userId),
            chatMessages: rows.messages,
            reads: rows.reads,
            sessions: rows.sessions,
            rootStatus: Dictionary(
                rows.sessions.filter { $0.parentId == nil }.map { ($0.id, $0.status) },
                uniquingKeysWith: { a, _ in a }
            ),
            rootStarter: Dictionary(
                rows.sessions.filter { $0.parentId == nil }.map { ($0.id, $0.userId) },
                uniquingKeysWith: { a, _ in a }
            )
        )
    }
}

/// EXP-1316: a thread row's trailing device glyph (`DeviceIconDisplay`) and
/// its accessibility label (the device's name).
struct ThreadRowDevice: Equatable, Sendable {
    let icon: String
    let name: String
}
