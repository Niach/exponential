import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-1291: the session tree as the phone's LISTS read it — ONE observation
/// of the synced `sessions` (their digests, never `messages`) /
/// `session_issues` / `pull_requests` rows, owned by
/// the navigator for its whole lifetime and handed down through the
/// environment, so the Chats tab, its dot, Home's Running band and the
/// board switcher's per-team dots never disagree. Every rule is ExpCore's
/// `SessionTree` (fixture-locked ×4); this model only maps rows and keeps
/// the derived lists.
@MainActor @Observable
final class SessionListsModel {
    /// Every team's chat groups (`partitionChats(teamId: nil)` — phones
    /// band the rows per team).
    private(set) var groups: [ChatsListGroup] = []
    /// Every team's agents (`listAgents(teamId: nil)`, NEW-2: the synced
    /// `agents` definitions).
    private(set) var agents: [AgentRow] = []
    /// NEW-2: my conversation session with each agent (a root with
    /// `agent_id` I started), by agent id; absent = none yet.
    private(set) var agentConversations: [String: String] = [:]
    /// #EXP-1733: each team's live apps (the synced `apps` rows, trash
    /// hidden), by team id: the Chats list's Apps section.
    private(set) var appsByTeam: [String: [Chats.ChatApp]] = [:]
    /// Home's Running band: my live chats — needs-you first, then running,
    /// each newest activity first.
    private(set) var running: [ChatsListRow] = []
    /// A chat's first linked issue identifier (the row's mono id).
    private(set) var identifiers: [String: String] = [:]
    /// A session's agent CLI (the row's run mark), by session id (EXP-1323:
    /// nested child rows too).
    private(set) var clis: [String: String] = [:]
    /// EXP-1309: a Chats row's quiet word off its pull requests, by root id
    /// (`SessionTree.chatRowWord`).
    private(set) var words: [String: ChatRowWord] = [:]
    /// NEW-41: each root's pull-request states (the Chats tab's lead glyph).
    private(set) var prStates: [String: [String]] = [:]
    /// EXP-1316: a chat row's device glyph (the ROOT's `device_id` against
    /// the synced devices rows), by root id; absent = no glyph.
    private(set) var devices: [String: ChatRowDevice] = [:]
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
    /// #FEED-191: `NeedsYou.rows`' input off the same rows, by team (its ages
    /// follow the clock, so the fold runs in the view).
    private(set) var needsYouInputs: [String: NeedsYouInput] = [:]
    /// #EXP-1714: the synced teams' ids and the viewer: their own rows of an
    /// unsynced (personal) team show in every team (`inTeamOrPersonal`).
    private(set) var syncedTeamIds: Set<String> = []
    private(set) var viewerId: String?

    @ObservationIgnored private var task: Task<Void, Never>?
    /// Bumped by every `apply`/`stop`: what `chatList`'s memo is valid for.
    @ObservationIgnored private var revision = 0
    @ObservationIgnored private var chatListMemo: [Chats.ChatListView: (key: String, list: Chats.ChatList)] = [:]
    @ObservationIgnored private var boundAccountId: String?

    nonisolated init() {}

    /// The Chats tab's amber dot: a chat waits on a person (the
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
        groups.contains { group in
            group.key == .needsYou && group.rows.contains { row in teamId.map { shows(row, in: $0) } ?? true }
        }
    }

    /// #EXP-1388 (`dotCases`): an archived chat never lights the dot.
    func chatsUnread(teamId: String?) -> Bool {
        Chats.chatsUnread(groups: teamGroups(teamId), reads: reads)
    }

    /// The Chats tab's list (`Chats.chatList`) of a team, computed once per
    /// sync emission, team, view and clock tick (a push over the list
    /// re-renders it without rebuilding the rows).
    func chatList(teamId: String?, view: Chats.ChatListView, now: String, utcOffsetMinutes: Int) -> Chats.ChatList {
        // The inputs are read either way, so the caller observes them.
        let groups = teamGroups(teamId), messages = chatMessages, reads = reads, prStates = prStates, words = words, sessions = sessions
        let key = "\(revision)|\(teamId ?? "")|\(view)|\(now)|\(utcOffsetMinutes)"
        if let memo = chatListMemo[view], memo.key == key { return memo.list }
        let list = Chats.chatList(
            groups: groups,
            messages: messages,
            reads: reads,
            prStates: prStates,
            words: words,
            view: view,
            now: now,
            utcOffsetMinutes: utcOffsetMinutes,
            // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
            visible: Set(sessions.map(\.id))
        )
        chatListMemo[view] = (key, list)
        return list
    }

    /// `partitionChats` narrowed to one team (empty groups dropped).
    func teamGroups(_ teamId: String?) -> [ChatsListGroup] {
        guard let teamId else { return groups }
        return groups.compactMap { group in
            let rows = group.rows.filter { shows($0, in: teamId) }
            return rows.isEmpty ? nil : ChatsListGroup(key: group.key, title: group.title, rows: rows)
        }
    }

    /// #EXP-1714: a row of `teamId`, or the viewer's own row of an unsynced
    /// (personal) team.
    private func shows(_ row: ChatsListRow, in teamId: String) -> Bool {
        AgentSettings.inTeamOrPersonal(
            teamId: row.teamId, userId: rootStarter[row.rootId], team: teamId,
            teamIds: syncedTeamIds, me: viewerId
        )
    }

    /// #FEED-191 (needs-you.json): every plan, question and ready PR waiting on
    /// the viewer in this team, newest first. #EXP-1714: plus the viewer's own
    /// sessions of an unsynced (personal) team.
    func needsYou(teamId: String?, me: String?, now: Date) -> [NeedsYou.Row] {
        guard let teamId else { return [] }
        var input = needsYouInputs[teamId] ?? NeedsYouInput()
        for (key, other) in needsYouInputs where key != teamId && !syncedTeamIds.isEmpty {
            let mine = other.sessions.filter {
                AgentSettings.inTeamOrPersonal(
                    teamId: key, userId: $0.userId, team: teamId, teamIds: syncedTeamIds, me: me
                )
            }
            guard !mine.isEmpty else { continue }
            let ids = Set(mine.map(\.id))
            input.sessions += mine
            input.messages += other.messages.filter { ids.contains($0.sessionId) }
            input.pullRequests += other.pullRequests.filter { ids.contains($0.rootSessionId ?? "") }
        }
        return NeedsYou.rows(
            now: now.timeIntervalSince1970 * 1000,
            me: me,
            sessions: input.sessions,
            messages: input.messages,
            pullRequests: input.pullRequests,
            mergeMode: input.mergeMode,
            owner: me.map { input.owners.contains($0) } ?? false
        )
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
        // before it can re-render the Chats tab, the tab bar and Home.
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
        revision += 1
        task?.cancel()
        task = nil
        boundAccountId = nil
        groups = []
        agents = []
        appsByTeam = [:]
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
        needsYouInputs = [:]
        syncedTeamIds = []
        viewerId = nil
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
        var apps: [AppEntity]
        /// #EXP-1710: each team's merge mode and its owners (Needs you's
        /// stuck rows and the approval-only ready rows).
        var mergeModes: [String: String] = [:]
        var owners: [String: Set<String>] = [:]
    }

    private struct Derived: Sendable, Equatable {
        var groups: [ChatsListGroup]
        var agents: [AgentRow]
        var agentConversations: [String: String]
        var appsByTeam: [String: [Chats.ChatApp]]
        var running: [ChatsListRow]
        var identifiers: [String: String]
        var clis: [String: String]
        var words: [String: ChatRowWord]
        var prStates: [String: [String]]
        var devices: [String: ChatRowDevice]
        var liveByTeam: [String: SessionTree.TeamLiveSessions]
        var chatMessages: [TreeMessage]
        var reads: [TreeSessionRead]
        var sessions: [TreeSession]
        var rootStatus: [String: String]
        var rootStarter: [String: String]
        var needsYouInputs: [String: NeedsYouInput]
        var syncedTeamIds: Set<String>
        var viewerId: String?
    }

    private func apply(_ derived: Derived) {
        revision += 1
        groups = derived.groups
        agents = derived.agents
        agentConversations = derived.agentConversations
        appsByTeam = derived.appsByTeam
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
        needsYouInputs = derived.needsYouInputs
        syncedTeamIds = derived.syncedTeamIds
        viewerId = derived.viewerId
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
        // Session e4a04d22 (chats.json LIST DATA): the lists never read a
        // transcript — each session's server-kept digest (#EXP-1730:
        // `sessions.digest`), turned back into messages
        // (`Chats.digestMessages`, `digestCases`), stands in for them, so
        // this observation never tracks `messages`.
        let digestMessages = sessions.flatMap { session -> [TreeMessage] in
            guard let digest = session.digest else { return [] }
            return Chats.digestMessages(sessionId: session.id, digest: digest, rootId: session.rootId)
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
            agents: try AgentDefinitionEntity.fetchAll(db).map(AgentLike.init),
            apps: try AppEntity.filter(Column("deleted_at") == nil).fetchAll(db),
            mergeModes: Dictionary(
                try TeamEntity.fetchAll(db).map { ($0.id, $0.mergeMode) }, uniquingKeysWith: { a, _ in a }
            ),
            owners: Dictionary(
                grouping: try TeamMemberEntity.filter(Column("role") == "owner").fetchAll(db), by: \.teamId
            ).mapValues { Set($0.map(\.userId)) }
        )
    }

    nonisolated private static func derive(_ rows: Rows, userId: String?) -> Derived {
        let groups = SessionTree.partitionChats(
            sessions: rows.sessions,
            sessionIssues: rows.sessionIssues,
            issues: rows.issues,
            messages: rows.messages,
            teamId: nil,
            reads: rows.reads,
            // NEW-1: the clock the Recent window reads (#EXP-1461: every
            // row is a root).
            now: ISO8601DateFormatter().string(from: Date())
        )
        // Roots holding a live session of mine (any depth of the tree).
        let mine = Set(rows.sessions.filter {
            $0.userId == userId && $0.status != DomainContract.sessionStatusEnded
        }.map(\.rootId))
        let running = groups
            .filter { $0.key != .recent }
            .flatMap(\.rows)
            .filter { mine.contains($0.sessionId) }
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
        // A stack edge may cross chats, never teams.
        let prsByTeam = Dictionary(grouping: rows.pullRequests, by: \.teamId)
        let prsByRoot = Dictionary(grouping: rows.pullRequests.filter { $0.rootSessionId != nil }) {
            $0.rootSessionId ?? ""
        }
        var words: [String: ChatRowWord] = [:]
        var prStates: [String: [String]] = [:]
        for row in groups.flatMap(\.rows) {
            let prs = prsByRoot[row.sessionId] ?? []
            words[row.sessionId] = SessionTree.chatRowWord(
                busy: row.busy,
                needsYou: row.needsYou,
                prs: prs,
                teamPrs: prsByTeam[row.teamId] ?? [],
                // #EXP-1703: on a person only because its device stayed away.
                offline: row.offline
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
        var devices: [String: ChatRowDevice] = [:]
        for row in groups.flatMap(\.rows) {
            guard let deviceId = row.deviceId, let candidates = deviceRowsById[deviceId] else { continue }
            guard let device = candidates.first(where: { $0.userId == starters[row.sessionId] }) ?? candidates.first
            else { continue }
            devices[row.sessionId] = ChatRowDevice(
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
            appsByTeam: Dictionary(grouping: rows.apps, by: \.teamId).mapValues { apps in
                apps.map {
                    Chats.ChatApp(appId: $0.id, slug: $0.slug, name: $0.name, icon: $0.icon, color: $0.color)
                }
            },
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
            ),
            needsYouInputs: needsYouInputs(rows),
            // #EXP-1714: `mergeModes` is keyed by every synced `teams` row.
            syncedTeamIds: Set(rows.mergeModes.keys),
            viewerId: userId
        )
    }

    /// #FEED-191: the fold's input per team, web `useNeedsYou`'s: a session is
    /// named by THE session title, an agent conversation by its agent; only
    /// the digests' waiting kinds.
    nonisolated private static func needsYouInputs(_ rows: Rows) -> [String: NeedsYouInput] {
        let titles = SessionTitle.titles(
            rows.sessions, sessionIssues: rows.sessionIssues, issues: rows.issues, messages: rows.messages
        )
        let agentNames = Dictionary(rows.agents.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
        let teamOf = Dictionary(rows.sessions.map { ($0.id, $0.teamId) }, uniquingKeysWith: { a, _ in a })
        var inputs: [String: NeedsYouInput] = [:]
        for session in rows.sessions {
            inputs[session.teamId, default: NeedsYouInput()].sessions.append(NeedsYou.SessionInput(
                id: session.id,
                rootId: session.rootId,
                name: session.agentId.flatMap { agentNames[$0] } ?? titles[session.id] ?? session.title ?? "",
                status: session.status,
                agentId: session.agentId,
                userId: session.userId,
                busy: session.busy
            ))
        }
        for message in rows.messages where ["plan", "question", "ask"].contains(message.card?.kind ?? "") {
            guard let teamId = teamOf[message.sessionId] else { continue }
            inputs[teamId, default: NeedsYouInput()].messages.append(NeedsYou.MessageInput(
                id: message.id,
                sessionId: message.sessionId,
                authorSessionId: message.authorSessionId,
                body: message.body,
                createdAt: message.createdAt,
                answeredAt: message.answeredAt,
                card: message.card
            ))
        }
        // #EXP-1710 (merge-train.json OFFLINE SINCE): the opener = the PR's
        // `sessionId` session, else its root; ExpCore's `offlineSince` reads
        // its device row (THE presence + `last_seen_at`). An opener without a
        // device = nil; an UNKNOWN last-seen (no device row synced, or none
        // recorded) = the PR's `updatedAt`, never online.
        let sessionDevice = Dictionary(
            rows.sessions.compactMap { s in s.deviceId.map { (s.id, (device: $0, user: s.userId)) } },
            uniquingKeysWith: { a, _ in a }
        )
        let devicesById = Dictionary(grouping: rows.deviceRows, by: \.deviceId)
        let clock = Date()
        func offlineSince(_ pr: PullRequestEntity) -> String? {
            let link = (pr.sessionId ?? pr.rootSessionId).flatMap { sessionDevice[$0] }
            let candidates = link.flatMap { devicesById[$0.device] } ?? []
            let device = candidates.first(where: { $0.userId == link?.user }) ?? candidates.first
            return MergeTrain.offlineSince(
                opener: link != nil,
                device: device.map {
                    MergeTrain.OpenerDevice(
                        online: DeviceLiveness.isOnline(lastSeenAt: $0.lastSeenAt, now: clock),
                        lastSeenAt: $0.lastSeenAt
                    )
                },
                updatedAt: pr.updatedAt
            )
        }
        for (teamId, mode) in rows.mergeModes {
            inputs[teamId, default: NeedsYouInput()].mergeMode = mode
        }
        for (teamId, owners) in rows.owners {
            inputs[teamId, default: NeedsYouInput()].owners = owners
        }
        for pr in rows.pullRequests {
            inputs[pr.teamId, default: NeedsYouInput()].pullRequests.append(NeedsYou.PullRequestInput(
                id: pr.id,
                rootSessionId: pr.rootSessionId,
                number: pr.number,
                title: pr.title ?? "",
                state: pr.state,
                checksState: pr.checksState,
                basePullRequestId: pr.basePullRequestId,
                createdAt: pr.createdAt,
                mergeState: pr.mergeState,
                mergeWait: pr.mergeWait,
                mergeHold: pr.mergeHold,
                deviceOfflineSince: offlineSince(pr)
            ))
        }
        return inputs
    }
}

/// #FEED-191: one team's `NeedsYou.rows` input.
struct NeedsYouInput: Equatable, Sendable {
    var sessions: [NeedsYou.SessionInput] = []
    var messages: [NeedsYou.MessageInput] = []
    var pullRequests: [NeedsYou.PullRequestInput] = []
    /// #EXP-1710: the team's merge mode (nil = not synced yet) and owners.
    var mergeMode: String?
    var owners: Set<String> = []
}

/// EXP-1316: a chat row's trailing device glyph (`DeviceIconDisplay`) and
/// its accessibility label (the device's name).
struct ChatRowDevice: Equatable, Sendable {
    let icon: String
    let name: String
}
