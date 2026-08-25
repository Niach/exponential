import Foundation

/// EXP-549/EXP-550: how a coding session's HOST MACHINE presents.
///
/// `coding_sessions.device_label` is a start-time SNAPSHOT — renaming the
/// machine afterwards never rewrites it, so the Agents list kept showing
/// `MacBook-Pro-von-Danny.local` long after the device became `macbook`
/// (EXP-549). The synced `devices` row is the live truth, and its
/// `last_seen_at` freshness is also what tells a viewer that the host went
/// away (lid closed): such a session is PAUSED, not ended — it resumes when
/// the machine comes back, so the clients must not draw a live dot or spin on
/// "waiting for the live stream" forever (EXP-550).
///
/// Pure and platform-shared: the desktop/web/Android mirrors resolve the same
/// way. Deliberately NOT a `CodingSessionDisplayState` case — that enum is
/// hand-mirrored ×4 and describes the RUN, while offline-ness is a property of
/// the machine hosting it.
public struct SessionDevicePresentation: Equatable {
    /// The name to show: the live devices row's label when one matched, else
    /// the session's own snapshot. nil when neither exists.
    public let label: String?
    /// The matched devices row stopped heartbeating (contract window). Always
    /// false when no devices row matched — an unknown machine is not evidence
    /// of an offline one.
    public let offline: Bool

    public init(label: String?, offline: Bool) {
        self.label = label
        self.offline = offline
    }

    /// Join the session to its live devices row and derive both fields.
    ///
    /// Match order: the row whose `deviceId` equals the session's stamped
    /// `device_id` (preferring the session owner's own row — two users may
    /// share a machine id through a shared server row) and, ONLY for rows
    /// written before that column existed, the UNIQUE row whose label still
    /// equals the snapshot (0 or 2+ matches = no row: an ambiguous label must
    /// never claim a machine is offline).
    public static func resolve(
        session: CodingSessionEntity,
        devices: [DeviceEntity],
        now: Date = Date()
    ) -> SessionDevicePresentation {
        let row = matchedRow(session: session, devices: devices)
        guard let row else {
            return SessionDevicePresentation(label: session.deviceLabel, offline: false)
        }
        let label = row.label.isEmpty ? session.deviceLabel : row.label
        return SessionDevicePresentation(
            label: label,
            offline: !DeviceLiveness.isOnline(lastSeenAt: row.lastSeenAt, now: now)
        )
    }

    private static func matchedRow(
        session: CodingSessionEntity,
        devices: [DeviceEntity]
    ) -> DeviceEntity? {
        if let deviceId = session.deviceId, !deviceId.isEmpty {
            let byId = devices.filter { $0.deviceId == deviceId }
            return byId.first { $0.userId == session.userId } ?? byId.first
        }
        guard let snapshot = session.deviceLabel, !snapshot.isEmpty else { return nil }
        let byLabel = devices.filter { $0.label == snapshot }
        return byLabel.count == 1 ? byLabel.first : nil
    }

    /// Whether the run reads "Paused" rather than live: the host is offline
    /// AND the run is still coding. A finished run (review/done) keeps its own
    /// state — its machine's presence stopped mattering.
    public func isPaused(_ state: CodingSessionDisplayState) -> Bool {
        guard offline else { return false }
        switch state {
        case .running, .needsInput: return true
        case .review, .done: return false
        }
    }

    /// The label every surface prints, with the same generic fallback the
    /// bylines already used before a device row (or snapshot) existed.
    public var displayLabel: String { label ?? "Desktop" }
}
