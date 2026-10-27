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

    /// A partial write (`Voice.Change`, incl. #NEW-73 language, voiceFile and
    /// voiceTranscript); the server applies `clearOnNone` and returns the row.
    public func update(accountId: String, change: Voice.Change) async throws -> Voice.Settings {
        try await trpc.mutation(accountId: accountId, path: "voice.update", input: change)
    }

    /// NEW-69: a phone's call with an agent (`platform: phone`); the server
    /// refuses with the availability reason, else answers the caller's relay
    /// ticket and the URL to dial as-is.
    public func startCall(accountId: String, agentId: String) async throws -> VoiceCall.Ticket {
        try await trpc.mutation(
            accountId: accountId,
            path: "voice.startCall",
            input: StartCallInput(agentId: agentId, platform: VoiceCall.Platform.phone.rawValue)
        )
    }
}

private struct StartCallInput: Encodable {
    let agentId: String
    let platform: String
}
