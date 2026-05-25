import Foundation

struct GoogleStatusResult: Decodable {
    let connected: Bool
    let connectedAt: String?
}

private struct EmptyIntegrationInput: Encodable {}

final class IntegrationsApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func googleStatus(accountId: String) async throws -> GoogleStatusResult {
        try await trpc.mutation(accountId: accountId, path: "integrations.google.status", input: EmptyIntegrationInput())
    }

    func googleDisconnect(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "integrations.google.disconnect", input: EmptyIntegrationInput())
    }

    func googleBackfill(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "integrations.google.backfill", input: EmptyIntegrationInput())
    }
}
