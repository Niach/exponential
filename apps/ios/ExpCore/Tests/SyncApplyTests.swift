import GRDB
import XCTest
@testable import ExpCore

// Tolerant partial-apply gate (the native sync-brick fix). A partial update that
// touches a column this build's schema doesn't have (e.g. the users shape
// delivering `onboarding_completed_at`) must NOT throw `no such column`, abort
// the batch transaction before the offset save, and refail forever. It must
// drop the unknown columns, apply the known subset, and — for a pure-unknown or
// composite-PK partial — no-op so the batch commits and the offset advances.
final class SyncApplyTests: XCTestCase {
    private var tempDir: URL!
    private var pool: DatabasePool!

    override func setUpWithError() throws {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("sync-apply-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        pool = try DatabasePool(path: tempDir.appendingPathComponent("t.sqlite").path)
        try DatabaseManager.runMigrations(on: pool)
    }

    override func tearDownWithError() throws {
        try? pool.close()
        pool = nil
        try? FileManager.default.removeItem(at: tempDir)
        tempDir = nil
    }

    // MARK: - Helpers

    private func seedUser(id: String, name: String?) throws {
        try pool.write { db in
            try UserEntity(
                id: id, name: name, email: "\(id)@example.com", image: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            ).save(db)
        }
    }

    private func userKey(_ id: String) -> String { #""public"."users"/"\#(id)""# }

    private func columns(_ dict: [String: Any]) -> Data {
        try! JSONSerialization.data(withJSONObject: dict)
    }

    private func fetchUser(_ id: String) throws -> UserEntity? {
        try pool.read { try UserEntity.fetchOne($0, key: id) }
    }

    private func seedIssue(id: String, title: String) throws {
        try pool.write { db in
            try IssueEntity(
                id: id, boardId: "p1", number: 1, identifier: "EXP-1", title: title,
                description: nil, status: "in_progress", priority: "none", assigneeId: nil,
                creatorId: "u1", source: nil, dueDate: nil, sortOrder: 1.0,
                completedAt: nil, duplicateOfId: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            ).save(db)
        }
    }

    private func issueKey(_ id: String) -> String { #""public"."issues"/"\#(id)""# }

    private func fetchIssue(_ id: String) throws -> IssueEntity? {
        try pool.read { try IssueEntity.fetchOne($0, key: id) }
    }

    private func fetchTeam(_ id: String) throws -> TeamEntity? {
        try pool.read { try TeamEntity.fetchOne($0, key: id) }
    }

    private func seedSubscriber(id: String, unsubscribed: Bool) throws {
        try pool.write { db in
            try IssueSubscriberEntity(
                id: id, issueId: "i1", userId: "u1", email: nil, teamId: "ws1",
                source: "manual", unsubscribed: unsubscribed,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            ).save(db)
        }
    }

    private func subscriberKey(_ id: String) -> String { #""public"."issue_subscribers"/"\#(id)""# }

    private func fetchSubscriber(_ id: String) throws -> IssueSubscriberEntity? {
        try pool.read { try IssueSubscriberEntity.fetchOne($0, key: id) }
    }

    // MARK: - Tests

    func testUnknownColumnPartialAppliesKnownSubset() async throws {
        try seedUser(id: "u1", name: "Old")
        // The users shape can deliver server-only columns absent from the native
        // schema — the known column must still apply, the unknown one drop.
        let message = ShapeMessage<UserEntity>.partialUpdate(
            key: userKey("u1"),
            columns: columns(["id": "u1", "name": "New", "onboarding_completed_at": "2026-05-01T00:00:00Z"])
        )
        try await applyBatch(messages: [message], name: "users", table: "users", pool: pool)
        XCTAssertEqual(try fetchUser("u1")?.name, "New")
    }

    func testPureUnknownPartialIsNoOpAndDoesNotThrow() async throws {
        try seedUser(id: "u2", name: "Keep")
        let message = ShapeMessage<UserEntity>.partialUpdate(
            key: userKey("u2"),
            columns: columns(["id": "u2", "onboarding_completed_at": "x", "had_trial": "true", "is_admin": "false"])
        )
        // Every SET column is unknown → nothing to update → no throw, row intact.
        try await applyBatch(messages: [message], name: "users", table: "users", pool: pool)
        XCTAssertEqual(try fetchUser("u2")?.name, "Keep")
    }

    func testCompositePkPartialIsSkipped() async throws {
        try await pool.write { db in
            try IssueLabelEntity(issueId: "i1", labelId: "l1", teamId: "ws1").save(db)
        }
        // issue_labels has a composite PK — a partial would emit a `WHERE id`
        // the table doesn't have. It must be skipped, not throw.
        let message = ShapeMessage<IssueLabelEntity>.partialUpdate(
            key: #""public"."issue_labels"/"i1"/"l1""#,
            columns: columns(["team_id": "ws2"])
        )
        try await applyBatch(messages: [message], name: "issue-labels", table: "issue_labels", pool: pool)
        let teamId = try await pool.read { db in
            try IssueLabelEntity.filter(Column("issue_id") == "i1").fetchOne(db)?.teamId
        }
        XCTAssertEqual(teamId, "ws1")
    }

    // EXP-481: a devices partial touching the jsonb launch_defaults column
    // (delivered as a raw JSON string on the partial wire) plus an unknown
    // future column must apply the known subset and advance — the shapes 17/18
    // decoders ride the same tolerant path as everything else.
    func testDevicePartialAppliesJsonbAndDropsUnknown() async throws {
        try await pool.write { db in
            try DeviceEntity(
                id: "row-1", userId: "u1", deviceId: "dev-1", label: "box",
                kind: "server", lastSeenAt: "2026-08-11T10:00:00Z"
            ).save(db)
        }
        let message = ShapeMessage<DeviceEntity>.partialUpdate(
            key: #""public"."devices"/"row-1""#,
            columns: columns([
                "id": "row-1",
                "launch_defaults": #"{"defaultAgent":"codex"}"#,
                "last_seen_at": "2026-08-11T10:05:00Z",
                "some_future_column": "x",
            ])
        )
        try await applyBatch(messages: [message], name: "devices", table: "devices", pool: pool)
        let row = try await pool.read { db in
            try DeviceEntity.fetchOne(db, key: "row-1")
        }
        XCTAssertEqual(row?.lastSeenAt, "2026-08-11T10:05:00Z")
        XCTAssertEqual(row?.launchDefaults, #"{"defaultAgent":"codex"}"#)
        XCTAssertEqual(row?.label, "box")
    }

    // FEED-33: `shared_team_ids` survives BOTH store paths — a full row goes
    // through GRDB's encoder (JSON text), a partial update stores the raw
    // Postgres literal — and reads back as the same id list either way.
    func testDeviceSharedTeamIdsRoundTripFullRowAndPartial() async throws {
        try await pool.write { db in
            try DeviceEntity(
                id: "row-1", userId: "u1", deviceId: "dev-1", label: "box",
                kind: "server", sharedTeamIds: ["team-1", "team-2"]
            ).save(db)
        }
        let stored = try await pool.read { db in
            try DeviceEntity.fetchOne(db, key: "row-1")
        }
        XCTAssertEqual(stored?.sharedTeamIds, ["team-1", "team-2"])

        let message = ShapeMessage<DeviceEntity>.partialUpdate(
            key: #""public"."devices"/"row-1""#,
            columns: columns(["id": "row-1", "shared_team_ids": "{team-3}"])
        )
        try await applyBatch(messages: [message], name: "devices", table: "devices", pool: pool)
        let patched = try await pool.read { db in
            try DeviceEntity.fetchOne(db, key: "row-1")
        }
        XCTAssertEqual(patched?.sharedTeamIds, ["team-3"])

        let cleared = ShapeMessage<DeviceEntity>.partialUpdate(
            key: #""public"."devices"/"row-1""#,
            columns: columns(["id": "row-1", "shared_team_ids": "{}"])
        )
        try await applyBatch(messages: [cleared], name: "devices", table: "devices", pool: pool)
        let privateRow = try await pool.read { db in
            try DeviceEntity.fetchOne(db, key: "row-1")
        }
        XCTAssertEqual(privateRow?.sharedTeamIds, [])
    }

    // EXP-481: the worktree `busy` BOOLEAN column takes the wire-bool mapping
    // on partials ("t" → real bool), like coding_sessions.needs_input.
    func testWorktreePartialCoercesBusyBool() async throws {
        try await pool.write { db in
            try DeviceWorktreeEntity(
                id: "wt-1", deviceRowId: "row-1", repoFullName: "acme/api",
                branch: "exp/EXP-1", busy: false
            ).save(db)
        }
        let message = ShapeMessage<DeviceWorktreeEntity>.partialUpdate(
            key: #""public"."device_worktrees"/"wt-1""#,
            columns: columns(["id": "wt-1", "busy": "t", "dirty": "tracked"])
        )
        try await applyBatch(
            messages: [message], name: "device-worktrees", table: "device_worktrees", pool: pool
        )
        let row = try await pool.read { db in
            try DeviceWorktreeEntity.fetchOne(db, key: "wt-1")
        }
        XCTAssertEqual(row?.busy, true)
        XCTAssertEqual(row?.dirty, "tracked")
    }

    func testBoardInsertPersistsRepositoryIconAndBranch() async throws {
        // The boards shape carries the repo ride-along, the curated icon and
        // the board's own branch (EXP-712); an inserted row must persist all
        // three (EXP-364 removed `is_protected` — protected boards are gone
        // from the product and the shape).
        let board = BoardEntity(
            id: "p1", teamId: "ws1", name: "Dogfood", slug: "exponential",
            prefix: "EXP", color: "#6366f1", sortOrder: 0,
            repositoryId: "repo1", icon: "rocket", defaultBranch: "release/1.x",
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<BoardEntity>.insert(
            key: #""public"."boards"/"p1""#, value: board
        )
        try await applyBatch(messages: [message], name: "boards", table: "boards", pool: pool)
        let stored = try await pool.read { try BoardEntity.fetchOne($0, key: "p1") }
        XCTAssertEqual(stored?.repositoryId, "repo1")
        XCTAssertEqual(stored?.icon, "rocket")
        XCTAssertEqual(stored?.defaultBranch, "release/1.x")
    }

    func testBoardInsertKeepsNullDefaultBranch() async throws {
        // NULL is the load-bearing value: the board follows its repo's default
        // branch (EXP-712). It must round-trip as nil, never as "".
        let board = BoardEntity(
            id: "p2", teamId: "ws1", name: "Design", slug: "design",
            prefix: "DES", color: "#6366f1", sortOrder: 0,
            repositoryId: nil, icon: nil, defaultBranch: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<BoardEntity>.insert(
            key: #""public"."boards"/"p2""#, value: board
        )
        try await applyBatch(messages: [message], name: "boards", table: "boards", pool: pool)
        let stored = try await pool.read { try BoardEntity.fetchOne($0, key: "p2") }
        XCTAssertNil(stored?.defaultBranch)
    }

    // EXP-1291: a sessions row off the Electric wire — bools as Postgres
    // "t"/"f", depth/sort_order as text, jsonb (guide, triggers,
    // action_inputs, blocked) as native JSON stored as TEXT, and a
    // server-only column leaking onto the wire ignored. It round-trips, a
    // partial flips the turn state (bool coercion) and an unknown column
    // drops, and a value-less delete removes the row by its key.
    func testSessionInsertPartialAndDeleteApply() async throws {
        let json = """
            {"id":"s1","team_id":"ws1","parent_id":null,"root_id":"s1","depth":"0",
             "kind":"thread","user_id":"u1","device_id":"dev-1","cli":"claude",
             "account":"p-1","model":"fable","effort":"high","plan_mode":"t",
             "computer_use":"f","ultracode":"f","dispatch":"t","repository_id":"r-1",
             "branch":"exp/s1","status":"running","busy":"t","needs_input":"f",
             "blocked":null,"started_at":"2026-10-13T09:00:00Z","last_turn_at":null,
             "ended_at":null,"shared":"t","title":null,
             "guide":[{"topic":"Summary","label":"Board","attachmentId":"a1"}],
             "action_id":"act-1","action_name":"Ship","action_inputs":{"repo":"r-1"},
             "spawned_by_session_id":null,"name":null,"icon":null,"color":null,
             "triggers":[],"sort_order":"0","instructions":"server only",
             "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
            """
        let session = try JSONDecoder().decode(SessionEntity.self, from: Data(json.utf8))
        XCTAssertTrue(session.isRoot)
        XCTAssertFalse(session.isAgent)
        XCTAssertTrue(session.isLive)
        XCTAssertTrue(session.planMode)
        XCTAssertTrue(session.dispatch)
        XCTAssertTrue(session.busy)
        XCTAssertTrue(session.shared)
        XCTAssertEqual(session.actionInputs, #"{"repo":"r-1"}"#)
        XCTAssertEqual(session.triggers, "[]")
        XCTAssertTrue(session.guide?.contains("\"attachmentId\":\"a1\"") ?? false)

        let key = #""public"."sessions"/"s1""#
        try await applyBatch(
            messages: [ShapeMessage<SessionEntity>.insert(key: key, value: session)],
            name: "sessions", table: "sessions", pool: pool
        )
        let stored = try await pool.read { try SessionEntity.fetchOne($0, key: "s1") }
        XCTAssertEqual(stored?.deviceId, "dev-1")
        XCTAssertEqual(stored?.repositoryId, "r-1")
        XCTAssertEqual(stored?.busy, true)
        XCTAssertEqual(stored?.dispatch, true)

        let partial = ShapeMessage<SessionEntity>.partialUpdate(
            key: key,
            columns: columns([
                "id": "s1", "status": "idle", "busy": "f", "needs_input": "t",
                "title": "Onboarding polish", "host_user_id": "u2",
            ])
        )
        try await applyBatch(messages: [partial], name: "sessions", table: "sessions", pool: pool)
        let patched = try await pool.read { try SessionEntity.fetchOne($0, key: "s1") }
        XCTAssertEqual(patched?.status, DomainContract.sessionStatusIdle)
        XCTAssertEqual(patched?.busy, false)
        XCTAssertEqual(patched?.needsInput, true)
        XCTAssertEqual(patched?.title, "Onboarding polish")
        XCTAssertEqual(patched?.planMode, true)

        try await applyBatch(
            messages: [ShapeMessage<SessionEntity>.delete(key: key, value: nil)],
            name: "sessions", table: "sessions", pool: pool
        )
        let gone = try await pool.read { try SessionEntity.fetchOne($0, key: "s1") }
        XCTAssertNil(gone)
    }

    // NEW-2: an agent's conversation session (`agent_id` set) is the agent
    // kind whatever the legacy `kind` shim says; a native-scalar (tRPC /
    // fixture) row with the defaulted columns absent takes the schema
    // defaults rather than throwing, and a row without `agent_id` (a
    // pre-NEW-2 server) is a thread even when its shim says `agent`.
    func testAgentSessionDecodesFacetsAndDefaults() throws {
        let json = """
            {"id":"ag1","team_id":"ws1","kind":"agent","agent_id":"def-1","user_id":"u1",
             "device_id":"dev-1","status":"idle","name":"Caretaker",
             "icon":"bot","color":"#4f46e5","sort_order":2,"triggers":[],
             "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
            """
        let agent = try JSONDecoder().decode(SessionEntity.self, from: Data(json.utf8))
        XCTAssertTrue(agent.isAgent)
        XCTAssertTrue(agent.traits.persistent)
        XCTAssertEqual(agent.agentId, "def-1")
        XCTAssertTrue(agent.isRoot)
        XCTAssertEqual(agent.rootId, "ag1")
        XCTAssertEqual(agent.depth, 0)
        XCTAssertEqual(agent.name, "Caretaker")
        XCTAssertEqual(agent.color, "#4f46e5")
        XCTAssertEqual(agent.sortOrder, 2)
        XCTAssertFalse(agent.busy)
        XCTAssertFalse(agent.shared)

        let legacy = try JSONDecoder().decode(SessionEntity.self, from: Data("""
            {"id":"ag2","team_id":"ws1","kind":"agent","user_id":"u1","device_id":"dev-1",
             "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
            """.utf8))
        XCTAssertNil(legacy.agentId)
        XCTAssertFalse(legacy.isAgent)
        XCTAssertFalse(legacy.traits.persistent)
    }

    // NEW-2: the three new shapes apply — an agent definition's triggers
    // (jsonb) stored as TEXT and its sort order off the wire as text, an
    // event's payload, a prompt; the server-only columns (`instructions`,
    // `trigger_state`, `body`) never land even when they leak onto the wire.
    func testAgentEventPromptShapesApply() async throws {
        let agent = try JSONDecoder().decode(AgentDefinitionEntity.self, from: Data("""
            {"id":"def-1","team_id":"ws1","name":"Triage","icon":"bot","color":"#22c55e",
             "triggers":[{"id":"tr1","enabled":true,"kind":"schedule","cron":"0 9 * * 1-5","tz":"Europe/Vienna"}],
             "device_id":"dev-1","cli":"claude","account":null,"model":"opus","owner_id":"u1",
             "sort_order":"2","instructions":"server only","trigger_state":{},
             "created_at":"2026-10-21T09:00:00Z","updated_at":"2026-10-21T09:00:00Z"}
            """.utf8))
        XCTAssertEqual(AgentTrigger.parseList(agent.triggers), [
            AgentTrigger(id: "tr1", when: .schedule(ScheduleTrigger(cron: "0 9 * * 1-5", tz: "Europe/Vienna"))),
        ])
        XCTAssertEqual(agent.sortOrder, 2)
        let event = try JSONDecoder().decode(EventEntity.self, from: Data("""
            {"id":"ev-1","team_id":"ws1","name":"pr.merged","payload":{"number":12},
             "emitted_by":"server","session_id":null,"user_id":null,
             "created_at":"2026-10-21T09:00:00Z"}
            """.utf8))
        XCTAssertEqual(event.payload, #"{"number":12}"#)
        let prompt = try JSONDecoder().decode(PromptEntity.self, from: Data("""
            {"id":"p-1","team_id":"ws1","name":"Review PR","icon":"rocket","sort_order":"1",
             "body":"server only","created_at":"2026-10-21T09:00:00Z","updated_at":"2026-10-21T09:00:00Z"}
            """.utf8))

        try await applyBatch(
            messages: [ShapeMessage<AgentDefinitionEntity>.insert(key: #""public"."agents"/"def-1""#, value: agent)],
            name: "agents", table: "agents", pool: pool
        )
        try await applyBatch(
            messages: [ShapeMessage<EventEntity>.insert(key: #""public"."events"/"ev-1""#, value: event)],
            name: "events", table: "events", pool: pool
        )
        try await applyBatch(
            messages: [ShapeMessage<PromptEntity>.insert(key: #""public"."prompts"/"p-1""#, value: prompt)],
            name: "prompts", table: "prompts", pool: pool
        )
        let stored = try await pool.read { db in
            (
                try AgentDefinitionEntity.fetchOne(db, key: "def-1"),
                try EventEntity.fetchOne(db, key: "ev-1"),
                try PromptEntity.fetchOne(db, key: "p-1")
            )
        }
        XCTAssertEqual(stored.0, agent)
        XCTAssertEqual(stored.0?.model, "opus")
        XCTAssertEqual(stored.1?.name, "pr.merged")
        XCTAssertEqual(stored.1?.emittedBy, "server")
        XCTAssertEqual(stored.2?.name, "Review PR")
        XCTAssertEqual(stored.2?.sortOrder, 1)

        // A partial renames the agent and replaces its triggers.
        let partial = ShapeMessage<AgentDefinitionEntity>.partialUpdate(
            key: #""public"."agents"/"def-1""#,
            columns: columns([
                "id": "def-1", "name": "Triage bot",
                "triggers": #"[{"id":"tr2","enabled":false,"kind":"event","name":"issue.created"}]"#,
            ])
        )
        try await applyBatch(messages: [partial], name: "agents", table: "agents", pool: pool)
        let patched = try await pool.read { try AgentDefinitionEntity.fetchOne($0, key: "def-1") }
        XCTAssertEqual(patched?.name, "Triage bot")
        XCTAssertEqual(AgentTrigger.parseList(patched?.triggers), [
            AgentTrigger(id: "tr2", enabled: false, when: .event(EventTrigger(name: "issue.created"))),
        ])
    }

    // NEW-3: a session's conversation row lands (the shape allowlist), and a
    // message carries its `conversation_id`; NEW-8: a legacy `session_id` on
    // the wire is ignored, the session is the conversation's.
    func testConversationAndMessageConversationIdApply() async throws {
        let conversation = try JSONDecoder().decode(ConversationEntity.self, from: Data("""
            {"id":"c-1","team_id":"ws1","kind":"session","session_id":"s1","title":null,
             "created_at":"2026-10-21T09:00:00Z","updated_at":"2026-10-21T09:00:00Z"}
            """.utf8))
        try await applyBatch(
            messages: [ShapeMessage<ConversationEntity>.insert(key: #""public"."conversations"/"c-1""#, value: conversation)],
            name: "conversations", table: "conversations", pool: pool
        )
        let message = try JSONDecoder().decode(MessageEntity.self, from: Data("""
            {"id":"m-c","session_id":"s1","root_id":"s1","team_id":"ws1","conversation_id":"c-1",
             "author_kind":"user","author_id":"u1","body":"hi",
             "created_at":"2026-10-21T09:00:00Z","updated_at":"2026-10-21T09:00:00Z"}
            """.utf8))
        try await applyBatch(
            messages: [ShapeMessage<MessageEntity>.insert(key: #""public"."messages"/"m-c""#, value: message)],
            name: "messages", table: "messages", pool: pool
        )
        let stored = try await pool.read { db in
            (try ConversationEntity.fetchOne(db, key: "c-1"), try MessageEntity.fetchOne(db, key: "m-c"))
        }
        XCTAssertEqual(stored.0, conversation)
        XCTAssertEqual(stored.0?.kind, DomainContract.conversationKindSession)
        XCTAssertEqual(stored.1?.conversationId, "c-1")
        let keyed = try await pool.read { try SessionQueries.messages(db: $0, sessionId: "s1") }
        XCTAssertEqual(keyed.map(\.id), ["m-c"])
        XCTAssertEqual(keyed.first?.sessionId, "s1")
    }

    // EXP-1291: the four child shapes round-trip — a message's card (jsonb)
    // as TEXT, a PR's number off the wire as text, a session issue's
    // defaulted source, a PR→issue link — and a partial answers a card.
    func testSessionChildShapesApply() async throws {
        let messageJson = """
            {"id":"m1","conversation_id":"c-s2","root_id":"s1","team_id":"ws1",
             "author_kind":"agent","author_id":null,"body":"",
             "card":{"kind":"question","question":"Which board?","options":["A","B"]},
             "answered_at":null,"consumed_at":null,
             "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
            """
        let message = try JSONDecoder().decode(MessageEntity.self, from: Data(messageJson.utf8))
        XCTAssertEqual(message.authorKind, DomainContract.messageAuthorKindAgent)
        XCTAssertTrue(message.card?.contains("\"kind\":\"question\"") ?? false)
        let messageKey = #""public"."messages"/"m1""#
        try await applyBatch(
            messages: [ShapeMessage<MessageEntity>.insert(key: messageKey, value: message)],
            name: "messages", table: "messages", pool: pool
        )
        try await applyBatch(
            messages: [ShapeMessage<MessageEntity>.partialUpdate(
                key: messageKey,
                columns: columns([
                    "id": "m1", "answered_at": "2026-10-13T09:05:00Z",
                    "card": #"{"kind":"question","question":"Which board?","answer":"A"}"#,
                ])
            )],
            name: "messages", table: "messages", pool: pool
        )
        let answered = try await pool.read { try MessageEntity.fetchOne($0, key: "m1") }
        XCTAssertEqual(answered?.answeredAt, "2026-10-13T09:05:00Z")
        XCTAssertTrue(answered?.card?.contains("\"answer\":\"A\"") ?? false)

        let prJson = """
            {"id":"pr1","team_id":"ws1","root_session_id":"s1","session_id":"s2",
             "repository_id":"r-1","number":"12","url":"https://github.com/acme/repo/pull/12",
             "title":"Polish","head":"exp/s1","base":"main","base_pull_request_id":null,
             "state":"open","merged_at":null,
             "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
            """
        let pr = try JSONDecoder().decode(PullRequestEntity.self, from: Data(prJson.utf8))
        XCTAssertEqual(pr.number, 12)
        XCTAssertTrue(pr.isOpen)
        try await applyBatch(
            messages: [ShapeMessage<PullRequestEntity>.insert(
                key: #""public"."pull_requests"/"pr1""#, value: pr
            )],
            name: "pull-requests", table: "pull_requests", pool: pool
        )

        let linkJson = """
            {"id":"si1","session_id":"s1","issue_id":"i1","team_id":"ws1",
             "added_by":"u1","added_by_session_id":null,"sort_order":"1",
             "assigned_session_id":"s2",
             "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
            """
        let link = try JSONDecoder().decode(SessionIssueEntity.self, from: Data(linkJson.utf8))
        XCTAssertEqual(link.source, DomainContract.sessionIssueSourceUser)
        XCTAssertEqual(link.sortOrder, 1)
        XCTAssertEqual(link.assignedSessionId, "s2")
        try await applyBatch(
            messages: [ShapeMessage<SessionIssueEntity>.insert(
                key: #""public"."session_issues"/"si1""#, value: link
            )],
            name: "session-issues", table: "session_issues", pool: pool
        )
        try await applyBatch(
            messages: [ShapeMessage<PullRequestIssueEntity>.insert(
                key: #""public"."pull_request_issues"/"pri1""#,
                value: PullRequestIssueEntity(
                    id: "pri1", pullRequestId: "pr1", issueId: "i1", teamId: "ws1",
                    createdAt: "2026-10-13T09:00:00Z", updatedAt: "2026-10-13T09:00:00Z"
                )
            )],
            name: "pull-request-issues", table: "pull_request_issues", pool: pool
        )
        let counts = try await pool.read { db in
            try ["messages", "pull_requests", "session_issues", "pull_request_issues"].map {
                try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM \($0)") ?? 0
            }
        }
        XCTAssertEqual(counts, [1, 1, 1, 1])
    }

    // EXP-1231: `draft_id` rides the issues shape: a row off the wire must
    // round-trip into the v63 column (a draft page watches for it), and a
    // snapshot that omits the key decodes as nil.
    func testIssueInsertPersistsDraftId() async throws {
        let json = """
            {"id":"i-drafted","board_id":"p1","number":"11","identifier":"EXP-11",
             "title":"From a draft","description":null,"status":"backlog","priority":"none",
             "assignee_id":null,"creator_id":"u1","source":"user","due_date":null,
             "sort_order":"1","completed_at":null,"duplicate_of_id":null,
             "estimate":null,
             "draft_id":"d-1",
             "created_at":"2026-10-07T09:00:00Z","updated_at":"2026-10-07T09:00:00Z"}
            """
        let issue = try JSONDecoder().decode(IssueEntity.self, from: Data(json.utf8))
        XCTAssertEqual(issue.draftId, "d-1")
        let message = ShapeMessage<IssueEntity>.insert(key: issueKey("i-drafted"), value: issue)
        try await applyBatch(messages: [message], name: "issues", table: "issues", pool: pool)
        XCTAssertEqual(try fetchIssue("i-drafted")?.draftId, "d-1")

        let older = """
            {"id":"i-undrafted","board_id":"p1","number":"12","identifier":"EXP-12",
             "title":"Plain","description":null,"status":"backlog","priority":"none",
             "assignee_id":null,"creator_id":"u1","source":"user","due_date":null,
             "sort_order":"2","completed_at":null,"duplicate_of_id":null,
             "created_at":"2026-10-07T09:00:00Z","updated_at":"2026-10-07T09:00:00Z"}
            """
        let bare = try JSONDecoder().decode(IssueEntity.self, from: Data(older.utf8))
        XCTAssertNil(bare.draftId)
    }

    // EXP-630: story points ride the issues shape — `estimate` arrives as
    // Postgres text and must land in the v52 integer column; a snapshot that
    // omits the key (a pre-EXP-630 server) decodes as nil, and an explicit
    // wire null too.
    func testIssueInsertPersistsEstimate() async throws {
        let json = """
            {"id":"i-est","board_id":"p1","number":"9","identifier":"EXP-9",
             "title":"Sized","description":null,"status":"backlog","priority":"none",
             "assignee_id":null,"creator_id":"u1","source":"user","due_date":null,
             "sort_order":"1","completed_at":null,"duplicate_of_id":null,
             "estimate":"5",
             "created_at":"2026-09-24T09:00:00Z","updated_at":"2026-09-24T09:00:00Z"}
            """
        let issue = try JSONDecoder().decode(IssueEntity.self, from: Data(json.utf8))
        XCTAssertEqual(issue.estimate, 5)
        let message = ShapeMessage<IssueEntity>.insert(key: issueKey("i-est"), value: issue)
        try await applyBatch(messages: [message], name: "issues", table: "issues", pool: pool)
        XCTAssertEqual(try fetchIssue("i-est")?.estimate, 5)

        // The partial-update path clears it back to NULL.
        let cleared = ShapeMessage<IssueEntity>.partialUpdate(
            key: issueKey("i-est"), columns: columns(["estimate": NSNull()])
        )
        try await applyBatch(messages: [cleared], name: "issues", table: "issues", pool: pool)
        XCTAssertNil(try fetchIssue("i-est")?.estimate)

        let older = """
            {"id":"i-unsized","board_id":"p1","number":"10","identifier":"EXP-10",
             "title":"Plain","description":null,"status":"backlog","priority":"none",
             "assignee_id":null,"creator_id":"u1","source":"user","due_date":null,
             "sort_order":"2","completed_at":null,"duplicate_of_id":null,
             "created_at":"2026-09-24T09:00:00Z","updated_at":"2026-09-24T09:00:00Z"}
            """
        XCTAssertNil(try JSONDecoder().decode(IssueEntity.self, from: Data(older.utf8)).estimate)
    }

    // EXP-630: the team's scale rides the teams shape into the v52 column.
    func testTeamInsertPersistsEstimationType() async throws {
        let json = """
            {"id":"t-est","name":"Acme","slug":"acme","icon_url":null,
             "estimation_type":"tshirt",
             "created_at":"2026-09-24T09:00:00Z","updated_at":"2026-09-24T09:00:00Z"}
            """
        let team = try JSONDecoder().decode(TeamEntity.self, from: Data(json.utf8))
        XCTAssertEqual(team.estimationType, "tshirt")
        let message = ShapeMessage<TeamEntity>.insert(key: #""public"."teams"/"t-est""#, value: team)
        try await applyBatch(messages: [message], name: "teams", table: "teams", pool: pool)
        XCTAssertEqual(try fetchTeam("t-est")?.estimationType, "tshirt")
    }

    // EXP-1105: yolo mode rides the teams shape into the v57 column.
    func testTeamInsertPersistsYoloMode() async throws {
        let json = """
            {"id":"t-yolo","name":"Acme","slug":"acme","icon_url":null,
             "yolo_mode":"t","estimation_type":null,
             "created_at":"2026-09-29T09:00:00Z","updated_at":"2026-09-29T09:00:00Z"}
            """
        let team = try JSONDecoder().decode(TeamEntity.self, from: Data(json.utf8))
        XCTAssertTrue(team.yoloMode)
        let message = ShapeMessage<TeamEntity>.insert(key: #""public"."teams"/"t-yolo""#, value: team)
        try await applyBatch(messages: [message], name: "teams", table: "teams", pool: pool)
        XCTAssertEqual(try fetchTeam("t-yolo")?.yoloMode, true)
    }

    // EXP-630: an emailed invite rides the team-invites shape carrying the
    // placeholder member it created; a pre-EXP-630 server omits the key (and a
    // link invite sends an explicit null), both of which decode as nil.
    // EXP-1076: `sent_at` rides too — set on every issued link, null on an
    // import seat nobody was ever invited.
    func testTeamInviteInsertPersistsPlaceholderUserId() async throws {
        let json = """
            {"id":"inv-1","team_id":"ws1","role":"member","email":"new@example.com",
             "placeholder_user_id":"u-placeholder","expires_at":"2026-10-01 10:00:00+00",
             "accepted_at":null,"sent_at":"2026-09-24 09:00:00+00",
             "created_at":"2026-09-24T09:00:00Z","updated_at":"2026-09-24T09:00:00Z"}
            """
        let invite = try JSONDecoder().decode(TeamInviteEntity.self, from: Data(json.utf8))
        XCTAssertEqual(invite.placeholderUserId, "u-placeholder")
        XCTAssertEqual(invite.sentAt, "2026-09-24 09:00:00+00")
        // The bearer token is excluded by the shape's columns allowlist.
        XCTAssertNil(invite.token)
        let message = ShapeMessage<TeamInviteEntity>.insert(
            key: #""public"."team_invites"/"inv-1""#, value: invite
        )
        try await applyBatch(
            messages: [message], name: "team-invites", table: "team_invites", pool: pool
        )
        let stored = try await pool.read { try TeamInviteEntity.fetchOne($0, key: "inv-1") }
        XCTAssertEqual(stored?.placeholderUserId, "u-placeholder")
        XCTAssertEqual(stored?.sentAt, "2026-09-24 09:00:00+00")

        // The claim lands as a partial update: accepted, binding cleared.
        let claimed = ShapeMessage<TeamInviteEntity>.partialUpdate(
            key: #""public"."team_invites"/"inv-1""#,
            columns: columns([
                "placeholder_user_id": NSNull(), "accepted_at": "2026-09-24 11:00:00+00",
            ])
        )
        try await applyBatch(
            messages: [claimed], name: "team-invites", table: "team_invites", pool: pool
        )
        let after = try await pool.read { try TeamInviteEntity.fetchOne($0, key: "inv-1") }
        XCTAssertNil(after?.placeholderUserId)
        XCTAssertEqual(after?.acceptedAt, "2026-09-24 11:00:00+00")

        let older = """
            {"id":"inv-2","team_id":"ws1","role":"member","email":null,
             "expires_at":"2026-10-01 10:00:00+00","accepted_at":null,
             "created_at":"2026-09-24T09:00:00Z","updated_at":"2026-09-24T09:00:00Z"}
            """
        let olderInvite = try JSONDecoder().decode(TeamInviteEntity.self, from: Data(older.utf8))
        XCTAssertNil(olderInvite.placeholderUserId)
        XCTAssertNil(olderInvite.sentAt)

        // EXP-1076: an import seat — bound to a placeholder, never mailed.
        let unsent = """
            {"id":"inv-3","team_id":"ws1","role":"member","email":"found@example.com",
             "placeholder_user_id":"u-import","expires_at":"2026-09-24 09:00:00+00",
             "accepted_at":null,"sent_at":null,
             "created_at":"2026-09-24T09:00:00Z","updated_at":"2026-09-24T09:00:00Z"}
            """
        let unsentInvite = try JSONDecoder().decode(TeamInviteEntity.self, from: Data(unsent.utf8))
        XCTAssertNil(unsentInvite.sentAt)
        try await applyBatch(
            messages: [ShapeMessage<TeamInviteEntity>.insert(
                key: #""public"."team_invites"/"inv-3""#, value: unsentInvite
            )],
            name: "team-invites", table: "team_invites", pool: pool
        )
        let storedUnsent = try await pool.read { try TeamInviteEntity.fetchOne($0, key: "inv-3") }
        XCTAssertEqual(storedUnsent?.placeholderUserId, "u-import")
        XCTAssertNil(storedUnsent?.sentAt)
    }

    // EXP-778: a pins row off the wire — `sort_order` arrives as Postgres
    // text like every numeric column, the two unused target columns as null.
    func testPinInsertDecodesWireSortOrderAndPersists() async throws {
        let json = """
            {"id":"pin1","user_id":"u1","team_id":"ws1","kind":"issue",
             "issue_id":"i1","session_id":null,"action_id":null,"sort_order":"1.5",
             "created_at":"2026-09-11T09:00:00Z","updated_at":"2026-09-11T09:00:00Z"}
            """
        let pin = try JSONDecoder().decode(PinEntity.self, from: Data(json.utf8))
        XCTAssertEqual(pin.sortOrder, 1.5)
        XCTAssertEqual(pin.targetId, "i1")
        XCTAssertNil(pin.sessionId)
        let message = ShapeMessage<PinEntity>.insert(key: #""public"."pins"/"pin1""#, value: pin)
        try await applyBatch(messages: [message], name: "pins", table: "pins", pool: pool)
        let stored = try await pool.read { try PinEntity.fetchOne($0, key: "pin1") }
        XCTAssertEqual(stored?.kind, DomainContract.pinKindIssue)
        XCTAssertEqual(stored?.issueId, "i1")
        XCTAssertEqual(stored?.sortOrder, 1.5)
        // A partial (the server reorders) coerces the numeric string too.
        let partial = ShapeMessage<PinEntity>.partialUpdate(
            key: #""public"."pins"/"pin1""#,
            columns: columns(["id": "pin1", "sort_order": "3"])
        )
        try await applyBatch(messages: [partial], name: "pins", table: "pins", pool: pool)
        let reordered = try await pool.read { try PinEntity.fetchOne($0, key: "pin1") }
        XCTAssertEqual(reordered?.sortOrder, 3)
    }

    // EXP-878: an issue_drafts row off the wire — `label_ids` is a Postgres
    // uuid[] text literal (`{a,b}`), the nullable picks arrive as JSON null,
    // and the stored [String] reads back through GRDB's JSON encoding.
    func testIssueDraftInsertDecodesWireLabelIdsAndPersists() async throws {
        let json = """
            {"id":"d1","user_id":"u1","team_id":"ws1","board_id":"b1",
             "title":"Prefetch avatars","description":"body","status_id":null,
             "priority":"high","assignee_id":null,
             "label_ids":"{11111111-1111-1111-1111-111111111111,22222222-2222-2222-2222-222222222222}",
             "due_date":"2026-10-01",
             "created_at":"2026-09-14T09:00:00Z","updated_at":"2026-09-14T09:30:00Z"}
            """
        let draft = try JSONDecoder().decode(IssueDraftEntity.self, from: Data(json.utf8))
        XCTAssertEqual(draft.labelIds, [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
        ])
        XCTAssertNil(draft.statusId)
        XCTAssertEqual(draft.priority, "high")
        XCTAssertEqual(draft.dueDate, "2026-10-01")

        let message = ShapeMessage<IssueDraftEntity>.insert(
            key: #""public"."issue_drafts"/"d1""#, value: draft
        )
        try await applyBatch(
            messages: [message], name: "issue-drafts", table: "issue_drafts", pool: pool
        )
        let stored = try await pool.read { try IssueDraftEntity.fetchOne($0, key: "d1") }
        XCTAssertEqual(stored?.title, "Prefetch avatars")
        XCTAssertEqual(stored?.labelIds.count, 2)
        XCTAssertNil(stored?.assigneeId)

        // A PARTIAL update (the user re-titles the draft elsewhere) carries
        // only the touched columns — the tolerant decoder must not need the
        // rest, and the raw uuid[] literal must still read back as a list.
        let partial = ShapeMessage<IssueDraftEntity>.partialUpdate(
            key: #""public"."issue_drafts"/"d1""#,
            columns: columns([
                "id": "d1", "title": "Renamed", "label_ids": "{33333333-3333-3333-3333-333333333333}",
            ])
        )
        try await applyBatch(
            messages: [partial], name: "issue-drafts", table: "issue_drafts", pool: pool
        )
        let patched = try await pool.read { try IssueDraftEntity.fetchOne($0, key: "d1") }
        XCTAssertEqual(patched?.title, "Renamed")
        XCTAssertEqual(patched?.labelIds, ["33333333-3333-3333-3333-333333333333"])
        // Untouched columns survive the partial.
        XCTAssertEqual(patched?.priority, "high")
        XCTAssertEqual(patched?.dueDate, "2026-10-01")
    }

    // EXP-1291: `issue_drafts.session_id` — a "New issue in this thread"
    // draft keeps its thread across a partial that does not touch it, and a
    // partial that nulls it (the thread was deleted: FK SET NULL) clears it.
    func testIssueDraftSessionIdPersistsAndSurvivesPartials() async throws {
        let json = """
            {"id":"d2","user_id":"u1","team_id":"ws1","board_id":"b1",
             "title":"In a thread","priority":"none","label_ids":"{}",
             "session_id":"t1",
             "created_at":"2026-10-12T09:00:00Z","updated_at":"2026-10-12T09:00:00Z"}
            """
        let draft = try JSONDecoder().decode(IssueDraftEntity.self, from: Data(json.utf8))
        XCTAssertEqual(draft.sessionId, "t1")
        try await applyBatch(
            messages: [ShapeMessage<IssueDraftEntity>.insert(
                key: #""public"."issue_drafts"/"d2""#, value: draft
            )],
            name: "issue-drafts", table: "issue_drafts", pool: pool
        )
        let stored = try await pool.read { try IssueDraftEntity.fetchOne($0, key: "d2") }
        XCTAssertEqual(stored?.sessionId, "t1")

        try await applyBatch(
            messages: [ShapeMessage<IssueDraftEntity>.partialUpdate(
                key: #""public"."issue_drafts"/"d2""#,
                columns: columns(["id": "d2", "title": "Renamed"])
            )],
            name: "issue-drafts", table: "issue_drafts", pool: pool
        )
        let renamed = try await pool.read { try IssueDraftEntity.fetchOne($0, key: "d2") }
        XCTAssertEqual(renamed?.title, "Renamed")
        XCTAssertEqual(renamed?.sessionId, "t1")

        try await applyBatch(
            messages: [ShapeMessage<IssueDraftEntity>.partialUpdate(
                key: #""public"."issue_drafts"/"d2""#,
                columns: columns(["id": "d2", "session_id": NSNull()])
            )],
            name: "issue-drafts", table: "issue_drafts", pool: pool
        )
        let cleared = try await pool.read { try IssueDraftEntity.fetchOne($0, key: "d2") }
        XCTAssertNil(cleared?.sessionId)
    }

    // EXP-878: a draft renders ONLY when its board resolves locally (the
    // shape is per-user and never trash-scoped, exactly like pins), and a
    // NULL `status_id` resolves to the team's Backlog builtin.
    func testResolvedDraftsHideMissingBoardsAndDefaultToBacklog() async throws {
        try await pool.write { db in
            try BoardEntity(
                id: "b1", teamId: "ws1", name: "Mobile App", slug: "mobile-app",
                prefix: "APP", color: "#4f46e5", sortOrder: 1, repositoryId: nil,
                createdAt: "2026-09-14T09:00:00Z", updatedAt: "2026-09-14T09:00:00Z"
            ).save(db)
            // The team's own backlog + one started custom.
            try IssueStatusEntity(
                id: "st-backlog", teamId: "ws1", category: "backlog", name: "Backlog",
                sortOrder: 0, builtinKey: "backlog",
                createdAt: "2026-09-14T09:00:00Z", updatedAt: "2026-09-14T09:00:00Z"
            ).save(db)
            try IssueStatusEntity(
                id: "st-review", teamId: "ws1", category: "started", name: "Reviewing",
                color: "#ff8800", sortOrder: 2,
                createdAt: "2026-09-14T09:00:00Z", updatedAt: "2026-09-14T09:00:00Z"
            ).save(db)
            // Newest edit first.
            try IssueDraftEntity(
                id: "d-old", userId: "u1", teamId: "ws1", boardId: "b1", title: "Older",
                createdAt: "2026-09-14T09:00:00Z", updatedAt: "2026-09-14T09:00:00Z"
            ).save(db)
            try IssueDraftEntity(
                id: "d-new", userId: "u1", teamId: "ws1", boardId: "b1", title: "",
                statusId: "st-review",
                createdAt: "2026-09-14T09:00:00Z", updatedAt: "2026-09-14T10:00:00Z"
            ).save(db)
            // Unresolvable: the board is not synced here (trashed / left team).
            try IssueDraftEntity(
                id: "d-gone", userId: "u1", teamId: "ws2", boardId: "b-missing",
                title: "Orphan",
                createdAt: "2026-09-14T09:00:00Z", updatedAt: "2026-09-14T11:00:00Z"
            ).save(db)
        }

        let rows = try await pool.read { db in try IssueDraftQueries.resolved(db: db) }
        XCTAssertEqual(rows.map(\.id), ["d-new", "d-old"])
        XCTAssertEqual(rows[0].status.rowId, "st-review")
        XCTAssertEqual(rows[0].board.name, "Mobile App")
        // NULL status_id = the team's Backlog builtin, never a bare fallback.
        XCTAssertEqual(rows[1].status.rowId, "st-backlog")
        XCTAssertEqual(rows[1].status.builtinKey, .backlog)

        // Narrowing to a team the drafts do not belong to yields nothing.
        let other = try await pool.read { db in
            try IssueDraftQueries.resolved(db: db, teamId: "ws2")
        }
        XCTAssertTrue(other.isEmpty)

        // The row itself is still readable by id — only the LIST hides it.
        let orphan = try await pool.read { db in
            try IssueDraftQueries.draft(db: db, id: "d-gone")
        }
        XCTAssertEqual(orphan?.title, "Orphan")
    }

    // EXP-1291: the session-tree reads — roots per kind (threads by
    // activity, agents by their order), a tree shallowest first, a root's
    // conversation and issues in order, the threads holding an issue, a
    // root's and an issue's pull requests, and my live sessions.
    func testSessionQueries() async throws {
        let at = "2026-10-13T09:00:00Z"
        try await pool.write { db in
            try SessionEntity(
                id: "t-old", teamId: "ws1", userId: "u1", deviceId: "dev-1",
                status: DomainContract.sessionStatusEnded, createdAt: at, updatedAt: at
            ).save(db)
            try SessionEntity(
                id: "t-new", teamId: "ws1", userId: "u1", deviceId: "dev-1",
                status: DomainContract.sessionStatusRunning, title: "Newest",
                createdAt: at, updatedAt: "2026-10-13T11:00:00Z"
            ).save(db)
            try SessionEntity(
                id: "t-child", teamId: "ws1", parentId: "t-new", rootId: "t-new", depth: 1,
                userId: "u2", deviceId: "dev-2", status: DomainContract.sessionStatusIdle,
                createdAt: "2026-10-13T10:00:00Z", updatedAt: "2026-10-13T10:00:00Z"
            ).save(db)
            try SessionEntity(
                id: "t-other", teamId: "ws2", userId: "u1", deviceId: "dev-1",
                createdAt: at, updatedAt: "2026-10-13T12:00:00Z"
            ).save(db)
            try SessionEntity(
                id: "ag-b", teamId: "ws1", kind: DomainContract.sessionKindAgent, userId: "u1",
                deviceId: "dev-1", status: DomainContract.sessionStatusIdle, name: "Beta",
                sortOrder: 1, createdAt: at, updatedAt: at, agentId: "def-b"
            ).save(db)
            try SessionEntity(
                id: "ag-a", teamId: "ws1", kind: DomainContract.sessionKindAgent, userId: "u1",
                deviceId: "dev-1", status: DomainContract.sessionStatusIdle, name: "Alpha",
                sortOrder: 1, createdAt: at, updatedAt: at, agentId: "def-a"
            ).save(db)
            // NEW-2: a legacy `kind = agent` row without `agent_id` is a
            // thread; the kind derives from `agent_id` alone.
            try SessionEntity(
                id: "t-legacy-kind", teamId: "ws3", kind: DomainContract.sessionKindAgent, userId: "u1",
                deviceId: "dev-1", createdAt: at, updatedAt: "2026-10-13T08:00:00Z"
            ).save(db)
            try AgentDefinitionEntity(
                id: "def-b", teamId: "ws1", name: "Beta", sortOrder: 1, createdAt: at, updatedAt: at
            ).save(db)
            try AgentDefinitionEntity(
                id: "def-a", teamId: "ws1", name: "Alpha", sortOrder: 1, createdAt: at, updatedAt: at
            ).save(db)
            try AgentDefinitionEntity(
                id: "def-z", teamId: "ws2", name: "Zed", sortOrder: 0, createdAt: at, updatedAt: at
            ).save(db)
            // NEW-8: messages key through their conversations.
            try ConversationEntity(
                id: "c-new", teamId: "ws1", sessionId: "t-new", createdAt: at, updatedAt: at
            ).save(db)
            try ConversationEntity(
                id: "c-child", teamId: "ws1", sessionId: "t-child", createdAt: at, updatedAt: at
            ).save(db)
            try MessageEntity(
                id: "m2", conversationId: "c-child", rootId: "t-new", teamId: "ws1",
                authorKind: DomainContract.messageAuthorKindAgent, body: "Done",
                createdAt: "2026-10-13T10:05:00Z", updatedAt: "2026-10-13T10:05:00Z"
            ).save(db)
            try MessageEntity(
                id: "m1", conversationId: "c-new", rootId: "t-new", teamId: "ws1",
                authorKind: DomainContract.messageAuthorKindUser, authorId: "u1", body: "Go",
                createdAt: at, updatedAt: at
            ).save(db)
            // i1 sits in BOTH ws1 threads; i2 only in t-new (linked first).
            try SessionIssueEntity(
                id: "si-b", sessionId: "t-new", issueId: "i1", teamId: "ws1", sortOrder: 2,
                createdAt: at, updatedAt: at
            ).save(db)
            try SessionIssueEntity(
                id: "si-a", sessionId: "t-new", issueId: "i2", teamId: "ws1", sortOrder: 1,
                createdAt: at, updatedAt: at
            ).save(db)
            try SessionIssueEntity(
                id: "si-c", sessionId: "t-old", issueId: "i1", teamId: "ws1",
                createdAt: at, updatedAt: at
            ).save(db)
            try PullRequestEntity(
                id: "pr1", teamId: "ws1", rootSessionId: "t-new", sessionId: "t-child",
                repositoryId: "r-1", number: 12, url: "https://github.com/acme/repo/pull/12",
                head: "exp/a", base: "main", createdAt: at, updatedAt: at
            ).save(db)
            try PullRequestIssueEntity(
                id: "pri1", pullRequestId: "pr1", issueId: "i1", teamId: "ws1",
                createdAt: at, updatedAt: at
            ).save(db)
        }
        try seedIssue(id: "i1", title: "First")
        try seedIssue(id: "i2", title: "Second")

        try await pool.read { db in
            XCTAssertEqual(
                try SessionQueries.roots(db: db, teamId: "ws1", kind: DomainContract.sessionKindThread)
                    .map(\.id),
                ["t-new", "t-old"]
            )
            XCTAssertEqual(
                try SessionQueries.roots(db: db, teamId: nil, kind: DomainContract.sessionKindThread)
                    .map(\.id),
                ["t-other", "t-new", "t-old", "t-legacy-kind"]
            )
            XCTAssertEqual(
                try SessionQueries.roots(db: db, teamId: "ws1", kind: DomainContract.sessionKindAgent)
                    .map(\.id),
                ["ag-a", "ag-b"]
            )
            XCTAssertEqual(try SessionQueries.agents(db: db, teamId: "ws1").map(\.id), ["def-a", "def-b"])
            XCTAssertEqual(try SessionQueries.agents(db: db, teamId: nil).map(\.id), ["def-z", "def-a", "def-b"])
            XCTAssertEqual(try SessionQueries.conversation(db: db, agentId: "def-a", userId: "u1")?.id, "ag-a")
            XCTAssertNil(try SessionQueries.conversation(db: db, agentId: "def-a", userId: "u2"))
            XCTAssertEqual(try SessionQueries.tree(db: db, rootId: "t-new").map(\.id), ["t-new", "t-child"])
            XCTAssertEqual(try SessionQueries.messages(db: db, rootId: "t-new").map(\.id), ["m1", "m2"])
            XCTAssertEqual(try SessionQueries.messages(db: db, sessionId: "t-child").map(\.id), ["m2"])
            XCTAssertEqual(
                try SessionQueries.sessionIssues(db: db, rootId: "t-new").map(\.id), ["si-a", "si-b"]
            )
            XCTAssertEqual(try SessionQueries.issues(db: db, rootId: "t-new").map(\.id), ["i2", "i1"])
            XCTAssertEqual(
                try SessionQueries.rootsHoldingIssue(db: db, issueId: "i1").map(\.id), ["t-new", "t-old"]
            )
            XCTAssertTrue(try SessionQueries.rootsHoldingIssue(db: db, issueId: "i-missing").isEmpty)
            XCTAssertEqual(try SessionQueries.pullRequests(db: db, rootId: "t-new").map(\.id), ["pr1"])
            XCTAssertEqual(try SessionQueries.pullRequests(db: db, issueId: "i1").map(\.id), ["pr1"])
            XCTAssertTrue(try SessionQueries.pullRequests(db: db, issueId: "i2").isEmpty)
            XCTAssertEqual(try SessionQueries.issueIds(db: db, pullRequestId: "pr1"), ["i1"])
        }
    }

    func testIssuelessNotificationInsertPersistsTeamId() async throws {
        // The notifications shape carries team_id — set on issue-less
        // agent_message / session_blocked rows (the helpdesk (gone, SLOP-4)
        // had its own kind here). An inserted row must round-trip both the
        // NULL issue_id and the team_id into the v2 column.
        let notification = NotificationEntity(
            id: "n1", userId: "u1", issueId: nil, teamId: "ws1",
            type: "agent_message", title: "Ada's agent: Build finished",
            body: "All green", readAt: nil, pushedAt: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
        let message = ShapeMessage<NotificationEntity>.insert(
            key: #""public"."notifications"/"n1""#, value: notification
        )
        try await applyBatch(messages: [message], name: "notifications", table: "notifications", pool: pool)
        let stored = try await pool.read { try NotificationEntity.fetchOne($0, key: "n1") }
        XCTAssertNil(stored?.issueId)
        XCTAssertEqual(stored?.teamId, "ws1")
        XCTAssertEqual(stored?.type, "agent_message")
    }

    // SLOP-4: a reporter's reply (author NULL, source reporter, audience
    // reporter) must persist — the v61 rebuild relaxed `author_id` and added
    // `audience` — and read back through the entity's flags.
    func testReporterCommentInsertPersistsNullAuthorAndAudience() async throws {
        let json = #"""
            {"id":"c-rep","issue_id":"i1","team_id":"ws1","author_id":null,
             "body":"Still broken on 17.4","kind":"regular","edited_at":null,
             "parent_id":null,"source":"reporter","audience":"reporter",
             "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}
            """#
        let comment = try JSONDecoder().decode(CommentEntity.self, from: Data(json.utf8))
        let message = ShapeMessage<CommentEntity>.insert(key: #""public"."comments"/"c-rep""#, value: comment)
        try await applyBatch(messages: [message], name: "comments", table: "comments", pool: pool)
        let stored = try await pool.read { try CommentEntity.fetchOne($0, key: "c-rep") }
        XCTAssertNil(stored?.authorId)
        XCTAssertEqual(stored?.isFromReporter, true)
        XCTAssertEqual(stored?.audience, "reporter")
    }

    func testPoisonedPartialDoesNotAbortBatch() async throws {
        try seedUser(id: "target", name: "Old")
        // A batch that used to abort at the poisoned partial (unknown column)
        // must now commit every message: both inserts land and the known part
        // of the partial applies.
        let messages: [ShapeMessage<UserEntity>] = [
            .insert(key: userKey("a"), value: UserEntity(
                id: "a", name: "A", email: "a@example.com", image: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            )),
            .partialUpdate(key: userKey("target"), columns: columns(["name": "Renamed", "bogus_col": "x"])),
            .insert(key: userKey("b"), value: UserEntity(
                id: "b", name: "B", email: "b@example.com", image: nil,
                createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
            )),
        ]
        try await applyBatch(messages: messages, name: "users", table: "users", pool: pool)
        XCTAssertNotNil(try fetchUser("a"))
        XCTAssertNotNil(try fetchUser("b"))
        XCTAssertEqual(try fetchUser("target")?.name, "Renamed")
    }

    // MARK: - Type-aware partial apply (the wire-decoding fix's apply half)

    func testPartialUpdateKeepsBooleanLookingTitleText() async throws {
        // Raw wire partials carry strings; a TEXT column must keep the exact
        // bytes. The old coercing pipeline turned "true" into the integer 1 and
        // "404" into the number 404 before binding — corrupting the title.
        for raw in ["true", "404"] {
            try seedIssue(id: "i1", title: "seed")
            let message = ShapeMessage<IssueEntity>.partialUpdate(
                key: issueKey("i1"), columns: columns(["title": raw])
            )
            try await applyBatch(messages: [message], name: "issues", table: "issues", pool: pool)
            XCTAssertEqual(try fetchIssue("i1")?.title, raw)
        }
    }

    func testPartialUpdateCoercesBooleanColumnFromWireString() async throws {
        // BOOLEAN columns are the affinity exception: a wire "true"/"t" must map
        // to a real Bool binding so GRDB's Bool read doesn't fail on TEXT.
        for raw in ["true", "t"] {
            try seedSubscriber(id: "s1", unsubscribed: false)
            let message = ShapeMessage<IssueSubscriberEntity>.partialUpdate(
                key: subscriberKey("s1"), columns: columns(["unsubscribed": raw])
            )
            try await applyBatch(
                messages: [message], name: "issue-subscribers",
                table: "issue_subscribers", pool: pool
            )
            XCTAssertEqual(try fetchSubscriber("s1")?.unsubscribed, true)
        }
    }

    func testPartialUpdateNumericStringsUseColumnAffinity() async throws {
        // INTEGER/REAL columns rely on SQLite affinity to convert numeric text.
        try seedIssue(id: "i2", title: "seed")
        let message = ShapeMessage<IssueEntity>.partialUpdate(
            key: issueKey("i2"), columns: columns(["number": "7", "sort_order": "3.5"])
        )
        try await applyBatch(messages: [message], name: "issues", table: "issues", pool: pool)
        let issue = try fetchIssue("i2")
        XCTAssertEqual(issue?.number, 7)
        XCTAssertEqual(issue?.sortOrder, 3.5)
    }
}
