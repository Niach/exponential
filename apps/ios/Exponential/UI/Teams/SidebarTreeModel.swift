import ExpCore
import Foundation
import GRDB

/// #EXP-2109 (sidebar-tree.json): what the Teams pages and the iPad sidebar
/// read beside `TeamState` (teams, boards) and `SessionListsModel` (sessions):
/// my synced `sidebar_layout` (section `team`), the agents and the apps, and
/// the teams I own, live off GRDB for the navigator's lifetime.
@MainActor @Observable
final class SidebarTreeModel {
    struct Agent: Equatable, Sendable {
        let id: String
        let teamId: String
        let name: String
        let icon: String
        let color: String
        let sortOrder: Double
        let role: String
    }

    struct App: Equatable, Sendable {
        let id: String
        let teamId: String
        let name: String
        let icon: String
        let color: String
        let agentId: String?
    }

    private struct Rows: Equatable, Sendable {
        var entries: [String: [SidebarFolders.Entry]] = [:]
        var agents: [Agent] = []
        var apps: [App] = []
        var owned: Set<String> = []
        var boardsSynced = false
    }

    private var rows = Rows()
    /// The first emission landed and the boards' first snapshot is in (else a
    /// team page would flash its empty state).
    private(set) var loaded = false

    @ObservationIgnored private var task: Task<Void, Never>?

    nonisolated init() {}

    func isOwner(_ teamId: String) -> Bool { rows.owned.contains(teamId) }
    var ownedTeams: Set<String> { rows.owned }

    func agent(_ id: String) -> Agent? { rows.agents.first { $0.id == id } }
    func app(_ id: String) -> App? { rows.apps.first { $0.id == id } }

    /// One page of a team's tree (`SidebarTree.phoneTeamPage`); nil = the
    /// folder is gone.
    func page(teamId: String, folderId: String?, boards: [BoardEntity], sessions: [TreeSession]) -> [SidebarTree.PhoneRow]? {
        let entries = rows.entries[teamId] ?? []
        return SidebarTree.phoneTeamPage(
            items: SidebarTree.teamItems(
                teamId: teamId,
                boards: boards.map { (id: $0.id, sortOrder: $0.sortOrder ?? 0) },
                // An app's own agent lists as its app only.
                agents: Chats.sidebarAgents(
                    rows.agents.filter { $0.teamId == teamId }, agentId: \.id, appAgentIds: rows.apps.map(\.agentId)
                ).map { SidebarTree.Agent(id: $0.id, sortOrder: $0.sortOrder, role: $0.role) },
                apps: rows.apps.filter { $0.teamId == teamId }.map { (id: $0.id, name: $0.name) },
                sessions: sessions.map { SidebarTree.Session(id: $0.id, teamId: $0.teamId, parentId: $0.parentId) },
                entries: entries
            ),
            entries: entries,
            folderId: folderId
        )
    }

    func folderName(teamId: String, folderId: String) -> String {
        rows.entries[teamId]?.first { $0.id == folderId }?.name ?? SidebarFolders.defaultFolderName
    }

    func bind(accountId: String, userId: String?, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> Rows in
            Rows(
                entries: Dictionary(
                    grouping: try SidebarLayoutEntity.filter(Column("section") == SidebarTree.section).fetchAll(db),
                    by: \.teamId
                ).mapValues { $0.compactMap(\.entry) },
                agents: try AgentDefinitionEntity.fetchAll(db).map {
                    Agent(
                        id: $0.id, teamId: $0.teamId, name: $0.name,
                        icon: $0.icon ?? SessionTree.ListCopy.agentDefaultIcon,
                        color: $0.color ?? SessionTree.ListCopy.agentDefaultColor,
                        sortOrder: $0.sortOrder, role: $0.role
                    )
                },
                apps: try AppEntity.filter(Column("deleted_at") == nil).fetchAll(db).map(AppListEntry.init(row:)).map {
                    App(
                        id: $0.id, teamId: $0.teamId ?? "", name: $0.name, icon: $0.glyph,
                        color: $0.color ?? AppGlyph.neutralColor, agentId: $0.agentId
                    )
                },
                owned: try userId.map { userId in
                    Set(try TeamMemberEntity
                        .filter(Column("user_id") == userId)
                        .filter(Column("role") == DomainContract.teamRoleOwner)
                        .fetchAll(db).map(\.teamId))
                } ?? [],
                boardsSynced: try ElectricOffset.fetchOne(db, key: "boards")?.isLive ?? false
            )
        }.removeDuplicates()
        task = Task { [weak self] in
            do {
                for try await next in observation.values(in: pool) {
                    guard let self else { return }
                    self.rows = next
                    self.loaded = next.boardsSynced
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        rows = Rows()
        loaded = false
    }
}
