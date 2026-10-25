import Foundation

// Mirrors apps/web/src/lib/trpc/voice.ts (NEW-67): the member's ONE voice
// settings row, server-only like the notification prefs (no shape).
public final class VoiceApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// The member's settings; the fixture's defaults when no row exists.
    public func get(accountId: String) async throws -> Voice.Settings {
        try await trpc.query(accountId: accountId, path: "voice.get")
    }

    /// A partial write; the server applies `clearOnNone` and returns the row.
    public func update(accountId: String, change: Voice.Change) async throws -> Voice.Settings {
        try await trpc.mutation(accountId: accountId, path: "voice.update", input: change)
    }
}
