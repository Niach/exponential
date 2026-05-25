import Foundation

struct EnsureDefaultResult: Decodable {
    let workspace: WorkspaceResult
}

struct WorkspaceResult: Decodable {
    let id: String
    let name: String
    let slug: String
}

struct UpdateWorkspaceInput: Encodable {
    let id: String
    var name: String?
    var isPublic: Bool?
    var publicWritePolicy: String?
    var iconUrl: String?
}

private struct EmptyInput: Encodable {}
private struct EmptyResult: Decodable {}

final class WorkspacesApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func ensureDefault(accountId: String) async throws -> WorkspaceResult {
        let result: EnsureDefaultResult = try await trpc.mutation(accountId: accountId, path: "workspaces.ensureDefault", input: EmptyInput())
        return result.workspace
    }

    func update(accountId: String, _ input: UpdateWorkspaceInput) async throws {
        let _: EmptyResult = try await trpc.mutation(accountId: accountId, path: "workspaces.update", input: input)
    }
}
