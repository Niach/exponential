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
    /// Whether the matched machine is heartbeating: true = inside the contract
    /// window, false = outside it, nil = UNKNOWN. Unknown covers both "no
    /// devices row matched" (an unknown machine is not evidence of an offline
    /// one) and, since EXP-656, "our devices cursor is older than the window,
    /// so a stale `last_seen_at` says nothing" — a phone that just woke up, or
    /// one with no network at all, must not claim a running machine is gone.
    public let online: Bool?

    /// Known to have stopped heartbeating. Unknown is NOT offline, so every
    /// consumer (and `isPaused`) reads the safe side by construction.
    public var offline: Bool { online == false }

    public init(label: String?, online: Bool?) {
        self.label = label
        self.online = online
    }

    /// Join the session to its live devices row and derive both fields.
    ///
    /// The ONLY join is `device_id`: the row whose `deviceId` equals the
    /// session's stamped one, preferring the session owner's own row (two users
    /// may share a machine id through a shared server row). The label-matching
    /// fallback for pre-EXP-549 rows is gone (EXP-560) — those rows have long
    /// since drained, and matching on a mutable display name could claim the
    /// wrong machine is offline. A session without a stamped `device_id`
    /// resolves no row and simply keeps its own snapshot label.
    ///
    /// - Parameter devicesFresh: whether our own `devices` shape has polled
    ///   within the contract window (EXP-656, `DeviceFreshness.isTrustworthy`).
    ///   A stale `last_seen_at` we haven't refreshed is ignorance, not
    ///   evidence, so it resolves to UNKNOWN instead of offline. Defaults to
    ///   true so unrelated call sites keep their pre-EXP-656 reading; the
    ///   surfaces that render presence pass it explicitly.
    public static func resolve(
        session: CodingSessionEntity,
        devices: [DeviceEntity],
        now: Date = Date(),
        devicesFresh: Bool = true
    ) -> SessionDevicePresentation {
        let row = matchedRow(session: session, devices: devices)
        guard let row else {
            return SessionDevicePresentation(label: session.deviceLabel, online: nil)
        }
        let label = row.label.isEmpty ? session.deviceLabel : row.label
        let live = DeviceLiveness.isOnline(lastSeenAt: row.lastSeenAt, now: now)
        return SessionDevicePresentation(
            label: label,
            online: live ? true : (devicesFresh ? false : nil)
        )
    }

    private static func matchedRow(
        session: CodingSessionEntity,
        devices: [DeviceEntity]
    ) -> DeviceEntity? {
        guard let deviceId = session.deviceId, !deviceId.isEmpty else { return nil }
        let byId = devices.filter { $0.deviceId == deviceId }
        return byId.first { $0.userId == session.userId } ?? byId.first
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
