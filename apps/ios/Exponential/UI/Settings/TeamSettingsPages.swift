import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// #EXP-1381 (fixtures/settings-nav.json): the Team page's phone rows, each
/// opening its OWN page (the row's label as the title, the native back) that
/// shows and observes only that category. The page of every section stacked
/// (and the scroll-to inside it) is gone.
enum TeamSettingsSection: String, Hashable, CaseIterable {
    case general, members, labels, boards, agents, repositories, prompts, devices

    init?(_ id: SettingsNav.Id) { self.init(rawValue: id.rawValue) }

    /// The generated row (label = the page's title, icon concept, count).
    var row: SettingsNav.Row { SettingsNav.rows.first { $0.id == rawValue }! }
}

/// The page a `teamSettingsSection` route opens.
struct TeamSettingsPage: View {
    let teamId: String
    let section: TeamSettingsSection

    var body: some View {
        switch section {
        case .general: TeamGeneralPage(teamId: teamId)
        case .members: TeamMembersPage(teamId: teamId)
        case .labels: TeamLabelsPage(teamId: teamId)
        case .boards: TeamBoardsPage(teamId: teamId)
        case .agents: TeamAgentsPage(teamId: teamId)
        case .repositories: TeamRepositoriesPage(teamId: teamId)
        case .prompts: TeamPromptsPage(teamId: teamId)
        // The devices page moved here from the phone tab bar (chats.json
        // `shell`): the same screen, titled "Devices" by itself.
        case .devices: AgentsView()
        }
    }
}

// MARK: - Shared

/// One settings page's chrome: the app background, the scrolling column, the
/// row's label as the inline title.
private struct TeamSettingsPageChrome<Content: View>: View {
    let section: TeamSettingsSection
    @ViewBuilder let content: Content

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    content
                }
                .padding(16)
            }
        }
        .navigationTitle(section.row.label)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityIdentifier(SettingsNav.pageTestId)
    }
}

private enum TeamPageQueries {
    /// The viewer OWNS the team (settings-nav.json `owner`).
    static func isOwner(_ db: Database, teamId: String, userId: String?) throws -> Bool {
        guard let userId else { return false }
        return try TeamMemberEntity
            .filter(Column("team_id") == teamId && Column("user_id") == userId)
            .filter(Column("role") == DomainContract.teamRoleOwner)
            .fetchCount(db) > 0
    }
}

/// Runs ONE tracked fetch (a page's whole data) and hands every distinct
/// value to `apply` until the calling `.task` is cancelled (the page left).
@MainActor
private func observePage<T: Equatable & Sendable>(
    _ deps: AppDependencies,
    accountId: String,
    _ fetch: @escaping @Sendable (Database) throws -> T,
    apply: (T) -> Void
) async {
    guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
    do {
        for try await value in ValueObservation.tracking(fetch).removeDuplicates().values(in: pool) {
            apply(value)
        }
    } catch {}
}

// MARK: - General

/// General = the team's identity + the Danger zone (delete team, owner only;
/// EXP-364: an owner may delete ANY team).
private struct TeamGeneralPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var team: TeamEntity?
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var data = Data()
    @State private var showDeleteTeam = false
    @State private var deletingTeam = false

    var body: some View {
        TeamSettingsPageChrome(section: .general) {
            if let team = data.team {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand("Team")
                    HStack(spacing: 12) {
                        TeamAvatar(team: team, size: 40)
                        Text(team.name)
                            .font(.body.weight(.medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Spacer()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .glassSection()
                }
                .accessibilityIdentifier("team-settings-identity")

                if data.isOwner {
                    dangerZone
                }
            }
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    team: try TeamEntity.fetchOne(db, key: teamId),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
        // EXP-1215: the app's own alert card (`GlassAlert`) on a zero-size
        // node of its own (EXP-240).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showDeleteTeam) {
                    GlassAlert(
                        prompt: Prompts.DeleteTeam.copy(name: data.team?.name ?? "this team"),
                        enabled: ["delete": !deletingTeam],
                        handlers: ["delete": { Task { await deleteTeam() } }]
                    )
                }
        }
    }

    /// ×4: the Danger zone heading is a filled band with its label in the
    /// danger red.
    private var dangerZone: some View {
        VStack(alignment: .leading, spacing: 8) {
            GlassSectionBand("Danger zone", danger: true)

            Button {
                showDeleteTeam = true
            } label: {
                HStack {
                    AppIcon(AppIcons.uiDelete, size: AppIcon.Size.medium)
                    Text("Delete team")
                }
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.red)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
                // Full-capsule hit target — .plain hit-tests only opaque pixels.
                .contentShape(Rectangle())
            }
            // NOT a GlassPill (EXP-698): a full-width destructive action in
            // the app's one red. EXP-1176: the row-radius rectangle.
            .background(
                GlassTokens.fillCard,
                in: RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
            )
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
            .buttonStyle(.plain)
            .accessibilityIdentifier("team-delete")
        }
    }

    private func deleteTeam() async {
        deletingTeam = true
        defer { deletingTeam = false }
        do {
            // An owner may delete ANY of their teams, including the last one
            // (EXP-188) — a team-less account lands on the zero-team state.
            try await deps.teamsApi.delete(accountId: accountId, teamId: teamId)
            // Membership changed, so every shape's where clause rotated —
            // relaunch the pipeline so all shapes re-scope immediately.
            await deps.syncManager.restartPipeline(accountId: accountId)
            await MainActor.run {
                // Pop the whole stack to root — parent views may still
                // target the deleted team.
                NotificationCenter.default.post(name: .teamDeleted, object: nil)
            }
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the team"),
                description: error.trpcUserMessage
            )
        }
    }
}

// MARK: - Members

/// Members (view/manage only — inviting by email stays web-only, EXP-216).
private struct TeamMembersPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var teamName: String?
        var members: [TeamMemberEntity] = []
        var users: [UserEntity] = []
        var invites: [TeamInviteEntity] = []
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data = Data()

    private var isOwner: Bool {
        guard let me = deps.auth.userId else { return false }
        return data.members.contains { $0.userId == me && $0.role == DomainContract.teamRoleOwner }
    }

    var body: some View {
        TeamSettingsPageChrome(section: .members) {
            TeamMembersSection(
                accountId: accountId,
                teamId: teamId,
                members: data.members,
                users: data.users,
                invites: data.invites,
                currentUserId: deps.auth.userId,
                membersApi: deps.teamMembersApi,
                isOwner: isOwner,
                teamName: data.teamName
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            await observePage(deps, accountId: accountId, { db in
                // Members ×4 list in JOIN order.
                let members = try TeamMemberEntity.filter(Column("team_id") == teamId)
                    .order(Column("created_at"), Column("id")).fetchAll(db)
                let userIds = members.map(\.userId)
                return Data(
                    teamName: try TeamEntity.fetchOne(db, key: teamId)?.name,
                    members: members,
                    users: try UserEntity.filter(userIds.contains(Column("id"))).fetchAll(db),
                    invites: try TeamInviteEntity.filter(Column("team_id") == teamId).fetchAll(db)
                )
            }, apply: { data = $0 })
        }
    }
}

// MARK: - Labels

private struct TeamLabelsPage: View {
    let teamId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var labels: [LabelEntity] = []

    var body: some View {
        TeamSettingsPageChrome(section: .labels) {
            TeamLabelsSection(
                accountId: accountId,
                teamId: teamId,
                labels: labels,
                labelsApi: deps.labelsApi
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            await observePage(deps, accountId: accountId, { db in
                try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }, apply: { labels = $0 })
        }
    }
}

// MARK: - Boards

/// Boards = the team's boards (a row opens the board's settings) + New board.
private struct TeamBoardsPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var boards: [BoardEntity] = []
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    // FEED-79: a failed board delete is a toast, never a rose line.
    @Environment(\.toaster) private var toaster
    @State private var data = Data()
    @State private var deleteBoardTarget: BoardEntity?
    @State private var deletingBoard = false

    var body: some View {
        TeamSettingsPageChrome(section: .boards) {
            TeamBoardsSection(
                boards: data.boards,
                accountId: accountId,
                teamId: teamId,
                isOwner: data.isOwner,
                boardsApi: deps.boardsApi,
                repositoriesApi: deps.repositoriesApi,
                onDelete: { board in deleteBoardTarget = board }
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    boards: try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $deleteBoardTarget) { board in
                    GlassAlert(
                        prompt: Prompts.TrashBoard.copy(name: board.name),
                        enabled: ["trash": !deletingBoard],
                        handlers: ["trash": { Task { await deleteBoard(board) } }]
                    )
                }
        }
    }

    private func deleteBoard(_ board: BoardEntity) async {
        deletingBoard = true
        defer { deletingBoard = false; deleteBoardTarget = nil }
        do {
            try await deps.teamsApi.deleteBoard(accountId: accountId, boardId: board.id)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the board"),
                description: error.trpcUserMessage
            )
        }
    }
}

// MARK: - Agents

/// Agents = the agents (a row opens the agent's settings; read-only for
/// members) + New agent, then the team's Events band (NEW-2).
private struct TeamAgentsPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var agents: [AgentDefinitionEntity] = []
        var isOwner = false
        var events: [EventEntity] = []
        var sessionTitles: [String: String] = [:]
        var users: [UserEntity] = []
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data = Data()
    @State private var agentTarget: AgentPageTarget?

    var body: some View {
        TeamSettingsPageChrome(section: .agents) {
            TeamAgentsSection(
                agents: data.agents,
                isOwner: data.isOwner,
                onOpen: { agentTarget = AgentPageTarget(agentId: $0) }
            )
            TeamEventsSection(events: data.events, sessionTitles: data.sessionTitles, users: data.users)
        }
        .navigationDestination(item: $agentTarget) { target in
            AgentSettingsPage(teamId: teamId, agentId: target.agentId, isOwner: data.isOwner)
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                let events = try EventEntity.filter(Column("team_id") == teamId)
                    .order(Column("created_at").desc, Column("id").desc)
                    .limit(TeamEventsSection.cap)
                    .fetchAll(db)
                let sessionIds = Array(Set(events.compactMap(\.sessionId)))
                let sessions = sessionIds.isEmpty
                    ? []
                    : try SessionEntity.filter(sessionIds.contains(Column("id"))).fetchAll(db)
                return Data(
                    agents: try SessionQueries.agents(db: db, teamId: teamId),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId),
                    events: events,
                    sessionTitles: Dictionary(
                        sessions.map { ($0.id, $0.title ?? "") }, uniquingKeysWith: { a, _ in a }
                    ),
                    users: try UserEntity.fetchAll(db)
                )
            }, apply: { data = $0 })
        }
    }
}

/// The agent page's navigation item: an agent id, or nil for a new one.
private struct AgentPageTarget: Hashable {
    let agentId: String?
}

// MARK: - Repositories

/// The repositories registry (server-only, read over tRPC). Member-visible
/// since EXP-557: any member connects/adds; row removal is sharer-or-owner.
private struct TeamRepositoriesPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var team: TeamEntity?
        var boards: [BoardEntity] = []
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data = Data()

    var body: some View {
        TeamSettingsPageChrome(section: .repositories) {
            TeamRepositoriesSection(
                accountId: accountId,
                team: data.team,
                currentUserId: deps.auth.userId,
                isOwner: data.isOwner,
                boards: data.boards,
                repositoriesApi: deps.repositoriesApi,
                integrationsApi: deps.integrationsApi,
                instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    team: try TeamEntity.fetchOne(db, key: teamId),
                    boards: try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
    }
}

// MARK: - Prompts

/// NEW-7: Prompts — a row opens the prompt's page (read-only for members);
/// "New prompt" is the owner's.
private struct TeamPromptsPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var prompts: [PromptEntity] = []
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var data = Data()

    var body: some View {
        TeamSettingsPageChrome(section: .prompts) {
            TeamPromptsSection(
                prompts: data.prompts,
                isOwner: data.isOwner,
                onOpen: { id in pushRoute(.prompt(accountId: accountId, teamId: teamId, id: id)) }
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    prompts: try PromptEntity.filter(Column("team_id") == teamId)
                        .order(Column("sort_order"), Column("name"))
                        .fetchAll(db),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
    }
}
