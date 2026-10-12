import Foundation

/// EXP-1259 (start surfaces): every remote start OTHER than the Agent
/// composer's draft start (`DraftStart`) carries ONE `startId` per PRESS: a
/// Resume, an account-switch resume. The press mints a fresh id, that press's
/// Retry (the failure toast's) re-sends the SAME id, so the server and the
/// device start it once (feedback-states.json `starting.dedupeMs`). A new
/// press is a new intent and mints again. Bound to its `target` (what the
/// press starts, on which machine, under which login) so a Retry can never
/// re-send an id minted for something else. Mirrored on Android
/// (`domain/StartIntent.kt`).
public struct StartIntent: Equatable, Sendable {
    public let startId: String
    public let target: String

    public init(startId: String, target: String) {
        self.startId = startId
        self.target = target
    }

    public static func mint() -> String { UUID().uuidString.lowercased() }

    /// A press: a new intent, a fresh id.
    public static func press(target: String, mint: () -> String = StartIntent.mint) -> StartIntent {
        StartIntent(startId: mint(), target: target)
    }

    /// A Retry of `pending`: the same id while it starts the same target,
    /// else (nothing pending, another target) a press.
    public static func retry(
        pending: StartIntent?,
        target: String,
        mint: () -> String = StartIntent.mint
    ) -> StartIntent {
        if let pending, pending.target == target { return pending }
        return press(target: target, mint: mint)
    }

    /// The target of a resume (`steer.startSession({ resumeSessionId })`):
    /// the run, its machine and the login it re-enters under (nil = the
    /// recorded one; a switch names another).
    public static func resumeTarget(sessionId: String, deviceId: String, account: String?) -> String {
        "resume:\(sessionId):\(deviceId):\(account ?? "")"
    }
}
