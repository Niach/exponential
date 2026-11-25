import AppIntents
import ExpCore
import Foundation
import GRDB

// #EXP-1640 (app-intents.json): Siri, Shortcuts, the Action button, Spotlight
// and Apple Intelligence. Two intents on the ACTIVE account, run in the app
// process through the app's own paths: `CreateIssueIntent` files a title
// through THE issues.create the New issue page uses (status = the board
// default) and answers without opening the app; `AskAgentIntent` sends the
// text into the member's conversation with an agent through THE composer's
// send path (`SessionsApi.send`: sealed on the phone when the tree has a
// key) and opens that conversation. Signed out = `copy.signedOut`.
// #EXP-1830: `OpenAgentChatIntent` opens an agent's conversation; every
// opening lands by app-shortcuts.json's back stack rule.

/// The app's dependencies for intents (set once at launch): an intent runs
/// in the app process, launched in the background when needed.
@MainActor
enum AppIntentsHost {
    static var deps: AppDependencies?

    /// The active account when it holds a session.
    struct Context: Sendable {
        let accountId: String
        let userId: String?
        let teamId: String?
        let deps: AppDependencies
    }

    /// The agents the "Ask <agent>" phrase knows.
    static func refreshShortcutParameters() {
        ExponentialShortcuts.updateAppShortcutParameters()
    }

    static func context() -> Context? {
        guard let deps, let accountId = deps.auth.activeAccountId,
              let account = deps.auth.accounts.first(where: { $0.id == accountId && $0.token != nil })
        else { return nil }
        return Context(
            accountId: accountId,
            userId: account.userId,
            teamId: SharedBoardMirror.readActiveTeam(accountId: accountId),
            deps: deps
        )
    }
}

private func dialog(_ text: String) -> IntentDialog {
    IntentDialog("\(text)")
}

/// A refusal the system speaks/shows as the intent's answer (the create's
/// answer opens the issue, so its other answers cannot be dialogs).
struct IntentMessage: Error, CustomLocalizedStringResourceConvertible {
    let text: String

    init(_ text: String) { self.text = text }

    var localizedStringResource: LocalizedStringResource { "\(text)" }
}

// MARK: - Boards

struct IntentBoard: AppIntents.AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Board"
    static let defaultQuery = IntentBoardQuery()

    let id: String
    let name: String
    let sortOrder: Double

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)")
    }

    /// The active account's boards (its active team's when it has one), in
    /// sidebar order.
    static func all(_ context: AppIntentsHost.Context) async -> [IntentBoard] {
        guard let pool = try? context.deps.db.pool(forAccountId: context.accountId) else { return [] }
        let rows = (try? await pool.read { db in try BoardEntity.fetchAll(db) }) ?? []
        let scoped = context.teamId.map { team in rows.filter { $0.teamId == team } }.flatMap { $0.isEmpty ? nil : $0 } ?? rows
        return scoped
            .map { IntentBoard(id: $0.id, name: $0.name, sortOrder: $0.sortOrder ?? 0) }
            .sorted { $0.sortOrder != $1.sortOrder ? $0.sortOrder < $1.sortOrder : $0.name < $1.name }
    }
}

struct IntentBoardQuery: EntityQuery {
    func entities(for identifiers: [String]) async throws -> [IntentBoard] {
        guard let context = await AppIntentsHost.context() else { return [] }
        return await IntentBoard.all(context).filter { identifiers.contains($0.id) }
    }

    func suggestedEntities() async throws -> [IntentBoard] {
        guard let context = await AppIntentsHost.context() else { return [] }
        return await IntentBoard.all(context)
    }
}

// MARK: - Agents

struct IntentAgent: AppIntents.AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Agent"
    static let defaultQuery = IntentAgentQuery()

    let id: String
    let name: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)")
    }

    /// agent-settings.json `orderedAgents` of the active account (its active
    /// team's when it has one): the member's Assistant first.
    static func ordered(_ context: AppIntentsHost.Context) async -> [AgentDefinitionEntity] {
        guard let pool = try? context.deps.db.pool(forAccountId: context.accountId) else { return [] }
        let rows = (try? await pool.read { db in try AgentDefinitionEntity.fetchAll(db) }) ?? []
        let scoped = context.teamId.map { team in rows.filter { $0.teamId == team } }.flatMap { $0.isEmpty ? nil : $0 } ?? rows
        return AgentOrdering.orderedAgents(scoped, me: context.userId)
    }
}

struct IntentAgentQuery: EntityStringQuery {
    func entities(for identifiers: [String]) async throws -> [IntentAgent] {
        guard let context = await AppIntentsHost.context() else { return [] }
        return await IntentAgent.ordered(context)
            .filter { identifiers.contains($0.id) }
            .map { IntentAgent(id: $0.id, name: $0.name) }
    }

    /// A spoken name: `matchIntentAgent` (exact, then prefix).
    func entities(matching string: String) async throws -> [IntentAgent] {
        guard let context = await AppIntentsHost.context() else { return [] }
        let agents = await IntentAgent.ordered(context)
        guard let id = AppIntentsModel.matchAgent(agents: agents, me: context.userId, spoken: string),
              let agent = agents.first(where: { $0.id == id }) else { return [] }
        return [IntentAgent(id: agent.id, name: agent.name)]
    }

    func suggestedEntities() async throws -> [IntentAgent] {
        guard let context = await AppIntentsHost.context() else { return [] }
        return await IntentAgent.ordered(context).map { IntentAgent(id: $0.id, name: $0.name) }
    }
}

// MARK: - Intents

struct CreateIssueIntent: AppIntent {
    static let title: LocalizedStringResource = "Create issue"
    static let openAppWhenRun = false

    @Parameter(title: "Title", requestValueDialog: IntentDialog("What's the issue?"))
    var issueTitle: String

    @Parameter(title: "Board")
    var board: IntentBoard?

    func perform() async throws -> some IntentResult & ProvidesDialog & OpensIntent {
        guard let context = await AppIntentsHost.context() else {
            throw IntentMessage(AppIntentsModel.Copy.signedOut)
        }
        guard let title = AppIntentsModel.issueTitle(issueTitle) else {
            throw $issueTitle.needsValueError(dialog(AppIntentsModel.Copy.askTitle))
        }
        let boards = await IntentBoard.all(context)
        let last = SharedBoardMirror.readLastUsed().flatMap { $0.accountId == context.accountId ? $0.boardId : nil }
        guard let boardId = AppIntentsModel.board(
            boards: boards.map { AppIntentsModel.Board(id: $0.id, sortOrder: $0.sortOrder) },
            lastBoardId: last,
            pickedId: board?.id
        ) else {
            throw IntentMessage(AppIntentsModel.Copy.noBoard)
        }
        let created = try await context.deps.issuesApi.create(
            accountId: context.accountId, CreateIssueInput(boardId: boardId, title: title)
        )
        // The answer opens THE issue page under this account.
        return .result(
            opensIntent: OpenIssueIntent(issueId: created.id, accountId: context.accountId),
            dialog: dialog(AppIntentsModel.Copy.created(identifier: created.issue?.identifier ?? title))
        )
    }
}

/// Opens THE issue page under the account that holds it (the created
/// issue's answer). Not offered on its own.
struct OpenIssueIntent: AppIntent {
    static let title: LocalizedStringResource = "Open issue"
    static let openAppWhenRun = true
    static let isDiscoverable = false

    @Parameter(title: "Issue")
    var issueId: String

    @Parameter(title: "Account")
    var accountId: String

    init() {}

    init(issueId: String, accountId: String) {
        self.issueId = issueId
        self.accountId = accountId
    }

    @MainActor
    func perform() async throws -> some IntentResult {
        guard let deps = AppIntentsHost.deps,
              deps.auth.accounts.contains(where: { $0.id == accountId && $0.token != nil }) else { return .result() }
        // #EXP-1830 OPENING: the back stack rule, under the account that holds it.
        if accountId == deps.auth.activeAccountId {
            deps.deepLinkBus.navigateToLaunch(.issue(issueId))
        } else {
            deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
        }
        return .result()
    }
}

struct AskAgentIntent: AppIntent {
    static let title: LocalizedStringResource = "Ask agent"
    /// The app opens the conversation.
    static let openAppWhenRun = true

    @Parameter(title: "Agent")
    var agent: IntentAgent?

    @Parameter(title: "Message")
    var text: String

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let context = AppIntentsHost.context() else {
            return .result(dialog: dialog(AppIntentsModel.Copy.signedOut))
        }
        let message = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty else { throw $text.needsValueError() }
        let agents = await IntentAgent.ordered(context)
        let agentId: String? = if let agent {
            agents.contains { $0.id == agent.id } ? agent.id : nil
        } else {
            AppIntentsModel.matchAgent(agents: agents, me: context.userId, spoken: "")
        }
        guard let agentId, let picked = agents.first(where: { $0.id == agentId }) else {
            return .result(dialog: dialog(AppIntentsModel.Copy.noAgent(name: agent?.name ?? "")))
        }
        _ = try await context.deps.sessionsApi.send(
            accountId: context.accountId, .agent(agentId: agentId, text: message)
        )
        // #EXP-1830 OPENING: THE agent's conversation over the Agent tab.
        context.deps.deepLinkBus.navigateToLaunch(.shortcut(.agent(agentId)))
        return .result(dialog: dialog(AppIntentsModel.Copy.sent(agent: picked.name)))
    }
}

/// #EXP-1830 (app-shortcuts.json OPENING): opens THE agent's conversation
/// (the caller's DM; none yet = the empty DM) by the back stack rule, the
/// same landing as the long-press menu's `agent:` entry. No agent picked =
/// the member's Assistant (`orderedAgents`' first).
struct OpenAgentChatIntent: AppIntent {
    static let title: LocalizedStringResource = "Open agent chat"
    static let openAppWhenRun = true

    @Parameter(title: "Agent")
    var agent: IntentAgent?

    @MainActor
    func perform() async throws -> some IntentResult {
        guard let context = AppIntentsHost.context() else {
            throw IntentMessage(AppIntentsModel.Copy.signedOut)
        }
        let agents = await IntentAgent.ordered(context)
        let agentId: String? = if let agent {
            agents.contains { $0.id == agent.id } ? agent.id : nil
        } else {
            AppIntentsModel.matchAgent(agents: agents, me: context.userId, spoken: "")
        }
        guard let agentId else {
            throw IntentMessage(AppIntentsModel.Copy.noAgent(name: agent?.name ?? ""))
        }
        context.deps.deepLinkBus.navigateToLaunch(.shortcut(.agent(agentId)))
        return .result()
    }
}

// MARK: - App Shortcuts

/// The fixture's `phrases`, every one naming the app.
struct ExponentialShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: CreateIssueIntent(),
            phrases: [
                "Create an issue in \(.applicationName)",
                "New \(.applicationName) issue",
            ],
            shortTitle: "Create issue",
            systemImageName: "plus.circle"
        )
        AppShortcut(
            intent: AskAgentIntent(),
            phrases: [
                "Ask \(\.$agent) in \(.applicationName)",
                "Ask \(.applicationName)",
            ],
            shortTitle: "Ask agent",
            systemImageName: "bubble.left"
        )
        AppShortcut(
            intent: OpenAgentChatIntent(),
            phrases: [
                "Open \(\.$agent) in \(.applicationName)",
                "Open \(.applicationName) agent chat",
            ],
            shortTitle: "Open agent chat",
            systemImageName: "bubble.left.and.bubble.right"
        )
    }
}
