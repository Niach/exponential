import Foundation

// Mirrors apps/web/src/lib/trpc/pins.ts (EXP-778) — the ONE pins endpoint:
// `pins.toggle` pins the target when absent and unpins it when present. The
// row itself arrives (or vanishes) over the `pins` shape; callers read
// pinned-ness from the local table, never from this result.
private struct PinToggleInput: Encodable {
    let teamId: String
    /// `issue` | `session` | `action` (contract `pinKind`).
    let kind: String
    let targetId: String
}

public struct PinToggleResult: Decodable, Sendable {
    /// The Electric txid the mutation committed under (nil on old servers).
    public let txId: Int?
    /// true = the call PINNED the target, false = it unpinned it.
    public let pinned: Bool

    public init(txId: Int?, pinned: Bool) {
        self.txId = txId
        self.pinned = pinned
    }
}

public final class PinsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Flip the caller's pin on one target. `kind` must be one of
    /// `DomainContract.pinKindValues`; `targetId` is the issue / coding
    /// session / action id it names. Builtin (`builtin:`) actions are not
    /// pinnable — the server refuses them, the UI never offers it.
    public func toggle(
        accountId: String,
        teamId: String,
        kind: String,
        targetId: String
    ) async throws -> PinToggleResult {
        try await trpc.mutation(
            accountId: accountId,
            path: "pins.toggle",
            input: PinToggleInput(teamId: teamId, kind: kind, targetId: targetId)
        )
    }
}
