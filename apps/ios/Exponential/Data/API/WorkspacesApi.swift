import Foundation

struct EnsureDefaultResult: Decodable {
    let workspace: WorkspaceResult
}

struct WorkspaceResult: Decodable {
    let id: String
    let name: String
    let slug: String
}

private struct EmptyInput: Encodable {}

final class WorkspacesApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func ensureDefault() async throws -> WorkspaceResult {
        let result: EnsureDefaultResult = try await trpc.mutation(path: "workspaces.ensureDefault", input: EmptyInput())
        return result.workspace
    }
}
