import ExpCore
import Foundation
import GRDB

/// #EXP-2109 (sidebar-tree.json): what the Teams tab and the iPad sidebar
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
        let slug: String
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

    /// The folder whose name is being edited in place (#EXP-2124 Rename; a
    /// new folder's, SHOWN while it has no item).
    var editingFolder: String?
    /// The team a link names: the Teams tab unfolds and scrolls to it.
    var revealTeam: String?
    /// The folds changed here (UserDefaults holds them; this redraws).
    private var folds = 0
    /// A write's layout until its rows sync.
    private var optimistic: [String: [SidebarFolders.Entry]] = [:]

    func entries(_ teamId: String) -> [SidebarFolders.Entry] { optimistic[teamId] ?? rows.entries[teamId] ?? [] }

    /// A team's one mixed list (`SidebarTree.teamItems`).
    func items(_ teamId: String, teamState: TeamState, lists: SessionListsModel) -> [SidebarTree.TeamItem] {
        SidebarTree.teamItems(
            teamId: teamId,
            boards: teamState.boards(of: teamId).map { (id: $0.id, sortOrder: $0.sortOrder ?? 0) },
            // An app's own agent lists as its app only.
            agents: Chats.sidebarAgents(
                rows.agents.filter { $0.teamId == teamId }, agentId: \.id, appAgentIds: rows.apps.map(\.agentId)
            ).map { SidebarTree.Agent(id: $0.id, sortOrder: $0.sortOrder, role: $0.role) },
            apps: rows.apps.filter { $0.teamId == teamId }.map { (id: $0.id, name: $0.name) },
            sessions: lists.sessions.map { SidebarTree.Session(id: $0.id, teamId: $0.teamId, parentId: $0.parentId) },
            entries: entries(teamId)
        )
    }

    /// THE tree (`SidebarTree.phoneTreeRows`) over this device's folds.
    func rows(teams: [TeamEntity], teamState: TeamState, lists: SessionListsModel) -> [SidebarTree.PhoneRow] {
        _ = folds
        return teams.flatMap { team -> [SidebarTree.PhoneRow] in
            let entries = entries(team.id)
            let closed = entries.filter { $0.kind == .folder && !SidebarFolders.isOpen(teamId: team.id, folderId: $0.id) }
            var out = SidebarTree.phoneTreeRows([.init(
                id: team.id, items: items(team.id, teamState: teamState, lists: lists), entries: entries,
                open: isTeamOpen(team.id), closedFolders: Set(closed.map(\.id))
            )])
            // The empty folder being named.
            if let editingFolder, isTeamOpen(team.id), !out.contains(where: { $0.id == editingFolder }),
               let folder = entries.first(where: { $0.id == editingFolder && $0.kind == .folder }) {
                out.append(.init(
                    kind: .folder, id: folder.id, teamId: team.id, depth: 1, name: folder.name, open: true, count: 0
                ))
            }
            return out
        }
    }

    /// The team's unfolded layout rows (Move to folder's targets).
    func folderRows(_ teamId: String, teamState: TeamState, lists: SessionListsModel) -> [SidebarFolders.Row] {
        SidebarFolders.rows(
            SidebarFolders.tree(items: items(teamId, teamState: teamState, lists: lists).map(\.id), entries: entries(teamId))
        ) { _ in true }
    }

    // MARK: - Folds (device-local; no flag = open)

    func isTeamOpen(_ teamId: String) -> Bool {
        UserDefaults.standard.object(forKey: SidebarTree.teamOpenKey(teamId)) as? Bool ?? true
    }

    func setTeamOpen(_ teamId: String, _ open: Bool) {
        UserDefaults.standard.set(open, forKey: SidebarTree.teamOpenKey(teamId))
        folds += 1
    }

    func setFolderOpen(_ folderId: String, teamId: String, _ open: Bool) {
        SidebarFolders.setOpen(open, teamId: teamId, folderId: folderId)
        folds += 1
    }

    // MARK: - Layout writes

    func save(_ writes: SidebarFolders.Writes, teamId: String, deps: AppDependencies, accountId: String) async throws {
        guard !writes.upserts.isEmpty || !writes.deletes.isEmpty else { return }
        try await optimistically(teamId, SidebarFolders.apply(writes, to: entries(teamId))) {
            try await deps.sidebarLayoutApi.save(accountId: accountId, teamId: teamId, writes)
        }
    }

    func rename(_ folderId: String, _ name: String, deps: AppDependencies, accountId: String) async throws {
        guard let teamId = rows.entries.keys.first(where: { entries($0).contains { $0.id == folderId } })
            ?? optimistic.keys.first(where: { entries($0).contains { $0.id == folderId } }) else { return }
        let next = entries(teamId).map { entry in
            var entry = entry
            if entry.id == folderId { entry.name = name }
            return entry
        }
        try await optimistically(teamId, next) {
            try await deps.sidebarLayoutApi.rename(accountId: accountId, id: folderId, name: name)
        }
    }

    func remove(_ targetId: String, teamId: String, deps: AppDependencies, accountId: String) async throws {
        try await optimistically(teamId, entries(teamId).filter { $0.targetId != targetId }) {
            try await deps.sidebarLayoutApi.remove(accountId: accountId, targetId: targetId)
        }
    }

    /// The result at once; dropped when the synced rows land (or the write fails).
    private func optimistically(
        _ teamId: String, _ next: [SidebarFolders.Entry], _ call: () async throws -> Void
    ) async throws {
        optimistic[teamId] = next
        do {
            try await call()
            // The rows sync right after; never keep a stale overlay.
            try? await Task.sleep(for: .seconds(3))
            if optimistic[teamId] == next { optimistic[teamId] = nil }
        } catch {
            optimistic[teamId] = nil
            throw error
        }
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
                        id: $0.id, teamId: $0.teamId ?? "", slug: $0.slug, name: $0.name, icon: $0.glyph,
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
                    for (teamId, entries) in next.entries where entries != self.rows.entries[teamId] {
                        self.optimistic[teamId] = nil
                    }
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
