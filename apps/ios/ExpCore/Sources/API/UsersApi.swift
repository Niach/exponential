import Foundation

// Personal API key management (Better Auth apikey plugin), mirrors
// apps/web/src/lib/trpc/users.ts. The desktop coding launcher writes the minted
// key into each worktree's `.mcp.json` so `claude` authenticates to the web MCP
// server as the REAL signed-in user (this replaces the deleted desktop-agent key).

/// The result of minting a key — the full raw `key` is returned exactly once.
public struct MintedApiKey: Decodable, Sendable {
    public let key: String    // full raw key — persisted once (written into .mcp.json)
    public let id: String     // key record id — used to revoke
    public let start: String? // visible key prefix (nullable server-side)

    public init(key: String, id: String, start: String?) {
        self.key = key
        self.id = id
        self.start = start
    }
}

/// A listed personal key (no raw secret). Decoded permissively since only the id
/// is load-bearing for revoke.
public struct PersonalApiKey: Decodable, Sendable, Identifiable {
    public let id: String
    public let name: String?
    public let start: String?
    public let createdAt: String?

    public init(id: String, name: String?, start: String?, createdAt: String?) {
        self.id = id
        self.name = name
        self.start = start
        self.createdAt = createdAt
    }

    enum CodingKeys: String, CodingKey { case id, name, start, createdAt }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        start = try c.decodeIfPresent(String.self, forKey: .start)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
    }
}

private struct NameInput: Encodable { let name: String? }
private struct KeyIdInput: Encodable { let id: String }
private struct ConfirmInput: Encodable { let confirm: Bool }

public final class UsersApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Mint a personal API key; the full `key` is returned once and must be stored
    /// then (it can't be re-fetched).
    public func mintPersonalApiKey(accountId: String, name: String? = nil) async throws -> MintedApiKey {
        try await trpc.mutation(
            accountId: accountId,
            path: "users.mintPersonalApiKey",
            input: NameInput(name: name)
        )
    }

    /// Server envelope: `users.listPersonalApiKeys` returns `{ keys: [...] }`.
    private struct PersonalApiKeysResult: Decodable {
        let keys: [PersonalApiKey]
    }

    /// The caller's existing personal keys (metadata only, no secret).
    public func listPersonalApiKeys(accountId: String) async throws -> [PersonalApiKey] {
        let result: PersonalApiKeysResult = try await trpc.query(
            accountId: accountId,
            path: "users.listPersonalApiKeys"
        )
        return result.keys
    }

    public func revokePersonalApiKey(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "users.revokePersonalApiKey",
            input: KeyIdInput(id: id)
        )
    }

    /// Permanently delete the signed-in user's account on this server —
    /// App Store guideline 5.1.1(v) (in-app account deletion). The server
    /// cascades sessions, memberships, authored content, and solo workspaces;
    /// the caller must follow up with local sign-out + cache wipe.
    public func deleteAccount(accountId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "users.deleteAccount",
            input: ConfirmInput(confirm: true)
        )
    }
}
