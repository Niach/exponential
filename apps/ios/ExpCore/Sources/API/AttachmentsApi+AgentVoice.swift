import Foundation

/// `POST /api/agents/{agentId}/voice-clip`'s answer.
public struct AgentVoiceClip: Decodable, Equatable, Sendable {
    public let clipId: String
    public let clipName: String
}

// Its own file: the share extension compiles `AttachmentsApi.swift` without the
// rest of ExpCore (`AgentSettings` included).
extension AttachmentsApi {
    /// #EXP-1847 (agent-settings.json `voice.clipUpload`): the agent's voice
    /// clip, one multipart part `file` (a WAV); the route stores it as the
    /// agent's attachment and the shape delivers `agents.voice` too.
    public func uploadAgentVoiceClip(
        accountId: String,
        agentId: String,
        data: Data,
        filename: String
    ) async throws -> AgentVoiceClip {
        let responseData = try await post(
            accountId: accountId,
            path: AgentSettings.VoiceRows.clipPath(agentId: agentId),
            data: data,
            filename: filename,
            contentType: "audio/wav",
            media: nil
        )
        return try JSONDecoder().decode(AgentVoiceClip.self, from: responseData)
    }

    /// Clears the agent's clip (back to the built-in voice).
    public func removeAgentVoiceClip(accountId: String, agentId: String) async throws {
        try await deleteRoute(accountId: accountId, path: AgentSettings.VoiceRows.clipPath(agentId: agentId))
    }
}
