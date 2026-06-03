import Foundation

public struct RegisterTokenInput: Encodable, Sendable {
    public let token: String
    public let platform: String

    public init(token: String, platform: String) {
        self.token = token
        self.platform = platform
    }
}

public struct UnregisterTokenInput: Encodable, Sendable {
    public let token: String

    public init(token: String) {
        self.token = token
    }
}

public final class PushTokensApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func register(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "pushTokens.register", input: RegisterTokenInput(token: token, platform: "ios"))
    }

    public func unregister(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "pushTokens.unregister", input: UnregisterTokenInput(token: token))
    }
}
