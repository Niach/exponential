import Foundation

struct RegisterTokenInput: Encodable {
    let token: String
    let platform: String
}

struct UnregisterTokenInput: Encodable {
    let token: String
}

final class PushTokensApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func register(token: String) async throws {
        try await trpc.mutationVoid(path: "pushTokens.register", input: RegisterTokenInput(token: token, platform: "ios"))
    }

    func unregister(token: String) async throws {
        try await trpc.mutationVoid(path: "pushTokens.unregister", input: UnregisterTokenInput(token: token))
    }
}
