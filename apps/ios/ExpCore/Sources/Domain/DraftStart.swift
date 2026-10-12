import Foundation

/// feedback-states.json `starting` (final review): ONE `startId` per DRAFT
/// START, bound to the target device + the draft fingerprint. Minted on the
/// first Start of a draft and reused by every Retry AND every later Start of
/// the SAME draft on the SAME device until a session lands or the draft or
/// device changes (then a fresh id). The server and the device dedupe the
/// id, so a slow device never ends up with two sessions. Mirrored on Android
/// (`domain/DraftStart.kt`).
public struct DraftStart: Equatable, Sendable {

    /// What a draft start is "the same" by: the text, the subject's issue /
    /// action ids and the attachments (stable per-pick ids, never the
    /// uploaded ones, which a send stamps).
    public struct Fingerprint: Equatable, Sendable {
        public let text: String
        public let issueIds: [String]
        public let actionId: String?
        public let attachments: [String]

        public init(text: String, issueIds: [String], actionId: String?, attachments: [String]) {
            self.text = text
            self.issueIds = issueIds
            self.actionId = actionId
            self.attachments = attachments
        }
    }

    public let startId: String
    public let deviceId: String
    public let fingerprint: Fingerprint

    public init(startId: String, deviceId: String, fingerprint: Fingerprint) {
        self.startId = startId
        self.deviceId = deviceId
        self.fingerprint = fingerprint
    }

    /// The start a Start or Retry of `fingerprint` on `deviceId` sends: the
    /// pending one while device and draft match, else a freshly minted one.
    public static func resolve(
        pending: DraftStart?,
        deviceId: String,
        fingerprint: Fingerprint,
        mint: () -> String = { UUID().uuidString.lowercased() }
    ) -> DraftStart {
        if let pending, pending.deviceId == deviceId, pending.fingerprint == fingerprint {
            return pending
        }
        return DraftStart(startId: mint(), deviceId: deviceId, fingerprint: fingerprint)
    }

    /// A landing of this start clears the draft and opens the session only
    /// when the draft is unchanged since its send; a draft the person changed
    /// after the timeout stays exactly as they left it (no navigation).
    public func landsInto(_ current: Fingerprint) -> Bool {
        fingerprint == current
    }
}
