import Foundation

/// #EXP-2109: the member's own sidebar layout (`sidebarLayout.*`): the writes
/// the pure `SidebarFolders` moves produce.
public final class SidebarLayoutApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    private struct Write: Encodable, Sendable {
        let id: String?
        let kind: String
        let targetId: String?
        let parentId: String?
        let name: String?
        let sortOrder: Double
    }

    private struct SaveInput: Encodable, Sendable {
        let teamId: String
        let section: String
        let upserts: [Write]
        let deletes: [String]
    }

    private struct RenameInput: Encodable, Sendable {
        let id: String
        let name: String
    }

    private struct RemoveInput: Encodable, Sendable {
        let targetId: String
    }

    public func save(accountId: String, teamId: String, _ writes: SidebarFolders.Writes) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "sidebarLayout.save",
            input: SaveInput(
                teamId: teamId,
                section: SidebarTree.section,
                upserts: writes.upserts.map {
                    Write(
                        id: $0.id, kind: $0.kind.rawValue, targetId: $0.targetId, parentId: $0.parentId, name: $0.name,
                        sortOrder: $0.sortOrder
                    )
                },
                deletes: writes.deletes
            )
        )
    }

    public func rename(accountId: String, id: String, name: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "sidebarLayout.rename", input: RenameInput(id: id, name: name))
    }

    /// `Remove from sidebar`.
    public func remove(accountId: String, targetId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "sidebarLayout.remove", input: RemoveInput(targetId: targetId))
    }
}
