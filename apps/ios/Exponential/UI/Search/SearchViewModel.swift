import ExpCore
import Foundation
import GRDB

/// Backs the Search page: observes every member team's issues (#EXP-2109:
/// no active team on a phone; search-list.json) and
/// ranks queries with the shared `IssueSearch` engine (EXP-892 — the ONE
/// algorithm web, iOS, Android and desktop run). A debounced server
/// `issues.search` over those teams (full-text over title + description +
/// comments) augments it with issues the local ranking missed.
///
/// TYPING never waits on the rank: every snapshot is PREPARED once
/// (`IssueSearch.Index`, built on GRDB's background queue), and a keystroke
/// ranks it in a debounced task off the main actor; the view only reads the
/// published `results`.
@MainActor @Observable
final class SearchViewModel {
    /// EXP-922: ONE flat ranked row — the web sheet's and the desktop
    /// palette's shape, in the single relevance order.
    struct Result: Identifiable, Sendable {
        let issue: IssueEntity
        var id: String { issue.id }
    }

    /// How many ranked issues the page renders at once, local + server hits —
    /// EXP-922: the ONE limit every search surface on every client uses.
    nonisolated private static let resultLimit = IssueSearch.defaultLimit
    /// The keystroke coalescing window before a rank runs.
    private static let rankDebounce: Duration = .milliseconds(120)
    /// The coalescing window before the server is asked.
    private static let serverDebounce: Duration = .milliseconds(250)

    /// What the page renders — always state, never computed in a body.
    private(set) var results: [Result] = []

    /// #NEW-133 (search-list.json SUGGESTION): the action row a recognised
    /// sentence puts on top, computed with the rank (off the main actor).
    private(set) var suggestion: SearchList.Suggestion?

    /// #NEW-136 (search-list.json TRANSLATE): the same row offering the model
    /// fallback for a sentence the grammar does not recognise. Never both:
    /// `offered`/`running` only while `suggestion` is nil.
    enum TranslateRow: Equatable {
        case hidden
        case offered
        /// The one call is running (the row's trailing spinner).
        case running
    }

    private(set) var translateRow: TranslateRow = .hidden

    /// A rank is still owed for the typed query (the debounce, the detached
    /// rank, or the index not built yet): the page keeps its rows and never
    /// shows the empty state meanwhile (feedback-states.json: no false
    /// "No issues match" while the work is pending).
    var isRanking: Bool {
        !query.isEmpty && (index == nil || localQuery != query)
    }

    /// Each team's statuses in render order, by board (EXP-1554: a row's
    /// glyph is its TEAM status's, e.g. the quarter clock of In Progress
    /// among three started statuses, as web draws it).
    private(set) var boardStatuses: [String: [ResolvedIssueStatus]] = [:]

    // #EDE-99 (search-list.json CHATS): the chat store's `searchAll`, the
    // synced roots only. nil `chatHits` = no store on this client.
    /// The Chats rows on screen (the previous query's while a newer one runs).
    private(set) var chatRows: [SearchList.GlobalChatRow] = []
    /// A newer query is with the store (the Section's spinner).
    private(set) var chatsLoading = false
    /// The client has a chat store (no store = today's list).
    let hasChatStore = ChatStoreHub.shared.isAvailable
    private var chatHits: [SearchList.ChatHitChat] = []
    private var chatHitsQuery = ""
    private var chatAuthors: [String: String] = [:]
    private var chatRoots: [String: SearchList.GlobalChatRoot] = [:]
    private var chatQuery = ""
    private var chatTask: Task<Void, Never>?

    private var query = ""
    /// The issues, prepared once per snapshot.
    private var index: IssueSearch.Index<IssueEntity>?
    /// The synced users (the rows that feed the creator names): the phrase
    /// grammar's members.
    private var members: [IssueSearchPhrase.Member] = []
    // The local rank for `localQuery` and the server's relevance-ordered
    // hits for `serverHitsQuery`; `publish` merges them only while both
    // belong to the current query, so a stale half never bleeds in.
    private var local: [IssueEntity] = []
    private var localQuery = ""
    private var serverHits: [SearchIssueHit] = []
    private var serverHitsQuery = ""

    private let accountId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private var issueTask: Task<Void, Never>?
    private var statusTask: Task<Void, Never>?
    private var rankTask: Task<Void, Never>?
    private var searchTask: Task<Void, Never>?
    /// #NEW-136: the on-device model and its one running call.
    private let translator: any IssueSearchTranslating
    private var translateTask: Task<Void, Never>?

    init(
        accountId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        translator: any IssueSearchTranslating = SearchTranslator.make()
    ) {
        self.accountId = accountId
        self.db = db
        self.issuesApi = issuesApi
        self.translator = translator
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
<<<<<<< HEAD
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
=======
        if teamId != self.teamId {
            self.teamId = teamId
            index = nil
            local = []
            localQuery = ""
            serverHits = []
            serverHitsQuery = ""
            results = []
            suggestion = nil
            endTranslation()
            members = []
            teamStatuses = []
            chatHits = []
            chatHitsQuery = ""
            chatRows = []
        }
        guard let teamId, let pool = try? db.pool(forAccountId: accountId) else { return }
>>>>>>> origin/master

        let statusObservation = ValueObservation.tracking { db -> [String: [ResolvedIssueStatus]] in
            let byTeam = Dictionary(grouping: try IssueStatusEntity.fetchAll(db), by: \.teamId)
                .mapValues(IssueStatusResolver.teamStatuses)
            return Dictionary(
                try BoardEntity.fetchAll(db).map { ($0.id, byTeam[$0.teamId] ?? []) }, uniquingKeysWith: { a, _ in a }
            )
        }
        .removeDuplicates()
        statusTask = Task { [weak self] in
            do {
                for try await next in statusObservation.values(in: pool) {
                    self?.boardStatuses = next
                }
            } catch {}
        }

        // The index is built in `map`, which GRDB runs off the main actor;
        // the box hands each build the previous one so unchanged rows keep
        // their prepared entry.
        let previous = IndexBox()
        // #NEW-21: each row carries its creator (the member's name + email,
        // the agent's name) so `by:<name>` filters and bare words match it;
        // #NEW-132: and its team status's name, so `status:<word>` matches a
        // custom status as well as the anchor.
        let observation = ValueObservation.tracking { db in
            let issues = try IssueEntity
                .filter(sql: "board_id IN (SELECT id FROM boards)")
                .fetchAll(db)
            let users = try UserEntity.fetchAll(db)
            let agents = try AgentDefinitionEntity.fetchAll(db)
            let statuses = try IssueStatusEntity.fetchAll(db)
            return SearchSnapshot(
                issues: issues,
                members: Dictionary(users.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }),
                agentNames: Dictionary(agents.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a }),
                statusNames: Dictionary(statuses.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
            )
        }
        .removeDuplicates()
        .map { snapshot in
            let index = IssueSearch.Index(snapshot.issues, reusing: previous.index) { issue in
                let member = issue.creatorId.flatMap { snapshot.members[$0] }
                return issue.searchRow(
                    creatorName: member?.name,
                    creatorEmail: member?.email,
                    creatorAgentName: issue.creatorAgentId.flatMap { snapshot.agentNames[$0] },
                    statusName: issue.statusId.flatMap { snapshot.statusNames[$0] }
                )
            }
            previous.index = index
            let members = snapshot.members.values
                .sorted { $0.id < $1.id }
                .map { IssueSearchPhrase.Member(name: $0.name, email: $0.email) }
            return (index: index, members: members)
        }
        issueTask = Task { [weak self] in
            do {
                for try await prepared in observation.values(in: pool) {
                    guard let self else { return }
                    self.index = prepared.index
                    self.members = prepared.members
                    // A synced change re-ranks the query on screen at once.
                    self.scheduleRank(debounce: false)
                }
            } catch {}
        }
        queryChanged(query)
    }

    func stopObserving() {
        issueTask?.cancel()
        issueTask = nil
        statusTask?.cancel()
        statusTask = nil
        rankTask?.cancel()
        rankTask = nil
        searchTask?.cancel()
        searchTask = nil
        chatTask?.cancel()
        chatTask = nil
        chatsLoading = false
        // #NEW-136: leaving the search ends a running translate call.
        endTranslation()
    }

    /// The view calls this on every keystroke. Both the rank and the server
    /// search are debounced and cancelled by the next keystroke.
    func queryChanged(_ query: String) {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        self.query = trimmed
        searchTask?.cancel()
        searchTask = nil
        searchChats(query)

        guard !trimmed.isEmpty else {
            rankTask?.cancel()
            rankTask = nil
            local = []
            localQuery = ""
            serverHits = []
            serverHitsQuery = ""
            results = []
            suggestion = nil
            endTranslation()
            return
        }
        // #NEW-136: a changed text ends a running translate call.
        if translateRow == .running { endTranslation() }
        scheduleRank(debounce: true)

        // ONE request over every synced team; failures just mean no server
        // hits.
        let accountId = accountId
        let api = issuesApi
        let db = db
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: Self.serverDebounce)
            guard !Task.isCancelled else { return }
            let teamIds = (try? await db.pool(forAccountId: accountId).read { db in
                try String.fetchAll(db, sql: #"SELECT "id" FROM "teams" LIMIT 50"#)
            }) ?? []
            guard !teamIds.isEmpty else { return }
            let hits = (try? await api.search(
                accountId: accountId,
                teamIds: teamIds,
                query: trimmed,
                limit: Self.resultLimit
            )) ?? []
            guard let self, !Task.isCancelled, self.query == trimmed else { return }
            self.serverHits = hits
            self.serverHitsQuery = trimmed
            self.publish()
        }
    }

    /// The synced roots (`SessionListsModel.searchRoots`): a change
    /// re-filters the rows on screen.
    func setChatRoots(_ roots: [String: SearchList.GlobalChatRoot]) {
        guard roots != chatRoots else { return }
        chatRoots = roots
        publishChats()
    }

    /// #EDE-99: the store is asked `debounceMs` after the last keystroke, in
    /// a detached task (never the main actor); a newer query drops the older
    /// answer, and the previous rows stay under the spinner meanwhile.
    private func searchChats(_ raw: String) {
        chatTask?.cancel()
        chatTask = nil
        let next = FindInChat.query(raw)
        chatQuery = next
        guard hasChatStore, !next.isEmpty else {
            chatsLoading = false
            chatHits = []
            chatHitsQuery = ""
            chatRows = []
            return
        }
        guard next != chatHitsQuery else {
            chatsLoading = false
            return
        }
        chatsLoading = true
        let accountId = accountId
        let db = db
        chatTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(SearchList.GlobalChats.debounceMs))
            guard !Task.isCancelled else { return }
            let (hits, authors) = await Task.detached(priority: .userInitiated) { () -> ([SearchList.ChatHitChat], [String: String]) in
                let hits = SearchList.chatHits(
                    ChatStoreHub.shared.searchAll(accountId: accountId, query: next, limit: FindInChat.hitsMax)
                )
                // A synced message's author leads a person's line with You.
                let ids = hits.compactMap { $0.hit.kind == "message" ? $0.hit.messageId : nil }
                guard !ids.isEmpty, let pool = try? db.pool(forAccountId: accountId) else { return (hits, [:]) }
                let authors = (try? pool.read { db in
                    try MessageEntity.filter(ids.contains(Column("id"))).fetchAll(db)
                }).map { rows in Dictionary(rows.map { ($0.id, $0.authorKind) }, uniquingKeysWith: { a, _ in a }) } ?? [:]
                return (hits, authors)
            }.value
            guard let self, !Task.isCancelled, self.chatQuery == next else { return }
            self.chatHits = hits
            self.chatAuthors = authors
            self.chatHitsQuery = next
            self.chatsLoading = false
            self.publishChats()
        }
    }

    private func publishChats() {
        guard !chatHitsQuery.isEmpty else { return }
        chatRows = SearchList.globalChatRows(
            query: chatHitsQuery,
            now: Date(),
            utcOffsetMinutes: TimeZone.current.secondsFromGMT() / 60,
            chats: chatHits,
            roots: chatRoots,
            messageAuthors: chatAuthors,
            limit: SearchList.GlobalChats.chatsMax
        )
    }

    /// #NEW-136: a pick of `Translate to a search`, the ONE place the model
    /// is called (typing never does). `done` gets the query the answer means
    /// (the view puts it in the field), or nil when there is none (the view
    /// toasts; the text stays as typed and the row is offered again).
    func translate(done: @escaping @MainActor (String?) -> Void) {
        guard translateRow == .offered else { return }
        let text = query
        let members = members
        let translator = translator
        translateRow = .running
        translateTask = Task { [weak self] in
            let translated = await IssueSearchTranslator.translate(text, members: members, translator: translator)
            guard let self, !Task.isCancelled, self.query == text, self.translateRow == .running else { return }
            self.translateTask = nil
            self.translateRow = translated == nil ? .offered : .hidden
            done(translated)
        }
    }

    /// Ends a running translate call and takes the row away.
    private func endTranslation() {
        translateTask?.cancel()
        translateTask = nil
        translateRow = .hidden
    }

    /// Ranks the current query over the prepared pool in a detached task
    /// (off the main actor), cancelling the previous rank.
    private func scheduleRank(debounce: Bool) {
        rankTask?.cancel()
        rankTask = nil
        let query = query
        guard !query.isEmpty, let index else { return }
        let members = members
        rankTask = Task { [weak self] in
            if debounce {
                try? await Task.sleep(for: Self.rankDebounce)
                guard !Task.isCancelled else { return }
            }
            let (ranked, suggestion, translatable) = await Task.detached(priority: .userInitiated) {
                let now = Date()
                let suggestion = SearchList.suggestion(text: query, members: members, now: now)
                return (
                    index.rank(query: query, limit: Self.resultLimit, now: now),
                    suggestion,
                    suggestion == nil && IssueSearchTranslation.wants(query, members: members, now: now)
                )
            }.value
            guard let self, !Task.isCancelled, self.query == query, self.index === index else { return }
            self.suggestion = suggestion
            // #NEW-136: offered only where the on-device model can answer;
            // a call already running for this text keeps its row.
            if self.translateRow != .running {
                self.translateRow = translatable && self.translator.isAvailable ? .offered : .hidden
            }
            self.local = ranked
            self.localQuery = query
            self.publish()
        }
    }

    /// EXP-892: the local rank first (an exact identifier hit, then undone
    /// before done, then identifier before title before description, recency
    /// breaking ties), then the server's full-text hits it missed (deduped by
    /// id, relevance order): a hit in the local store renders the local row,
    /// otherwise a slim row built from the returned fields. Capped at
    /// `resultLimit`; both halves are ≤ that, so this is cheap on the main
    /// actor.
    private func publish() {
        guard !query.isEmpty, localQuery == query else { return }
        let ranked: [IssueEntity]
        if serverHitsQuery == query, !serverHits.isEmpty {
            let index = index
            ranked = IssueSearch.mergeServerHits(
                local: local,
                hits: serverHits,
                limit: Self.resultLimit,
                localId: \.id,
                hitId: \.id,
                resolve: { hit in index?.value(id: hit.id) ?? Self.placeholderEntity(from: hit) }
            )
        } else {
            ranked = local
        }
        results = ranked.map { Result(issue: $0) }
    }

    /// A display-only stand-in for a server hit that has no local GRDB row
    /// (e.g. not yet synced) — carries exactly the fields the result row
    /// renders (priority, identifier, status, title) plus the ids needed for
    /// navigation.
    private static func placeholderEntity(from hit: SearchIssueHit) -> IssueEntity {
        IssueEntity(
            id: hit.id,
            boardId: hit.boardId,
            number: nil,
            identifier: hit.identifier,
            title: hit.title,
            description: nil,
            status: hit.status,
            priority: hit.priority,
            assigneeId: nil,
            creatorId: nil,
            source: nil,
            dueDate: nil,
            sortOrder: nil,
            completedAt: nil,
            duplicateOfId: nil,
            createdAt: "",
            updatedAt: ""
        )
    }
}

/// #NEW-21: one observed snapshot: the teams' issues and the synced members
/// and agents their creator ids resolve against (#NEW-132: and the team
/// status names their status ids resolve against).
private struct SearchSnapshot: Equatable, Sendable {
    let issues: [IssueEntity]
    let members: [String: UserEntity]
    let agentNames: [String: String]
    let statusNames: [String: String]
}

/// The previous snapshot's index, carried between `map` calls (GRDB runs
/// them one at a time on its own queue).
private final class IndexBox: @unchecked Sendable {
    var index: IssueSearch.Index<IssueEntity>?
}
