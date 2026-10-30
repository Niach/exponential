import Foundation

public struct RegisterTokenInput: Encodable, Sendable {
    public let token: String
    public let platform: String
    // EXP-1435: the relay capability this device minted for the server and
    // its X25519 push public key (base64url, no padding). Both or neither;
    // nil keys are omitted from the JSON (legacy register).
    public let capability: String?
    public let pushPublicKey: String?

    public init(token: String, platform: String, capability: String? = nil, pushPublicKey: String? = nil) {
        self.token = token
        self.platform = platform
        self.capability = capability
        self.pushPublicKey = pushPublicKey
    }
}

/// `pushTokens.relay`: the push relay base URL this server's devices register
/// with, or nil (no relay configured).
public struct PushRelayInfo: Decodable, Sendable {
    public let url: String?
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

    /// Registers with the relay capability + push public key: the server then
    /// seals this row's pushes and POSTs them to the relay (EXP-1435).
    public func register(accountId: String, token: String, capability: String, pushPublicKey: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "pushTokens.register",
            input: RegisterTokenInput(token: token, platform: "ios", capability: capability, pushPublicKey: pushPublicKey)
        )
    }

    /// The server's push relay URL; nil when it has none OR predates the
    /// procedure (404). Throws on transport/other failures.
    public func relayUrl(accountId: String) async throws -> String? {
        do {
            let info: PushRelayInfo = try await trpc.query(accountId: accountId, path: "pushTokens.relay")
            return info.url.flatMap { $0.isEmpty ? nil : $0 }
        } catch TrpcError.httpError(404, _) {
            return nil
        }
    }

    public func unregister(accountId: String, token: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "pushTokens.unregister", input: UnregisterTokenInput(token: token))
    }
}
