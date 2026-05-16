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

    func googleStatus() async throws -> GoogleStatusResult {
        try await trpc.mutation(path: "integrations.google.status", input: EmptyIntegrationInput())
    }

    func googleDisconnect() async throws {
        try await trpc.mutationVoid(path: "integrations.google.disconnect", input: EmptyIntegrationInput())
    }

    func googleBackfill() async throws {
        try await trpc.mutationVoid(path: "integrations.google.backfill", input: EmptyIntegrationInput())
    }
}
