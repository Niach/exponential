import Foundation

/// EXP-1252: moving a live SESSION to another device.
///
/// A move = ONE `sessions.send({ sessionId, deviceId })` with no text
/// (`SessionsSendInput.moveDevice`): the server writes the synced
/// `sessions.move_to_device_id` and the source host hands the SAME row over
/// after its current turn (worktree + conversation travel, logins stay).
/// Picking the session's CURRENT device while a move is pending cancels it
/// (the same call).
///
/// The session composer's options line shows the Device pin BEFORE Model,
/// built exactly like the launch line's device pin. Every sentence is
/// hand-mirrored ×4 (web `lib/session-device-move.ts`, desktop
/// `crates/ui/src/device_move.rs`, Android `domain/SessionDeviceMove.kt`).
public enum SessionDeviceMove {
    /// The picker sheet's headline.
    public static let pickerTitle = "Move to device"

    /// The pin while a move is pending; `{device}` = the target's name.
    public static let movingLabelTemplate = "Moving to {device}…"

    /// What a move does, said once where the move is offered.
    public static let note =
        "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device."

    /// A failed request, as a toast.
    public static let failureToast = "Couldn't move the session"

    // MARK: Refusals — one sentence each, shown ON the disabled control/row

    public static let reasonSteer = "Only people who can steer this session can move it."
    public static let reasonEnded = "This session has ended."
    public static let reasonAgent = "Agents move from Settings → Agents."
    public static let reasonAside = "An aside stays on its parent's device."
    public static let reasonOffline = "This device is offline."
    public static let reasonVisibility = "This device is shared differently, so the session can't move here."
    public static let reasonNoLogin = "This device has no signed-in login for this agent."
    /// The target runs a build too old to receive a session (web
    /// `REASON_UPDATE`).
    public static let reasonUpdate = "This device needs an update to receive sessions."

    /// `Moving to <device>…`.
    public static func movingLabel(_ device: String) -> String {
        movingLabelTemplate.replacingOccurrences(of: "{device}", with: device)
    }

    /// The pin exists at all: never on an agent (a persistent kind moves
    /// from Settings → Agents) nor on an aside (it stays on its parent's
    /// device).
    public static func pinVisible(kind: String, aside: Bool) -> Bool {
        !SessionKind.traits(kind).persistent && !aside
    }

    /// The whole pin's refusal, or nil = the picker opens. Steer first: a
    /// viewer who may not steer never learns more.
    public static func blockedReason(canSteer: Bool, ended: Bool) -> String? {
        if !canSteer { return reasonSteer }
        if ended { return reasonEnded }
        return nil
    }

    /// Whether `device` is shared with `teamId` (the session's team).
    public static func sharedWithTeam(_ device: SteerDevice, teamId: String) -> Bool {
        device.sharedTeamIds.contains(teamId)
    }

    /// The first desktop/CLI build that receives a handoff (the server's
    /// `MOVE_MIN_VERSION`, web `lib/sessions/move.ts`).
    public static let moveMinVersion = (0, 14, 73)

    /// Whether a device's reported marketing version is at least
    /// `moveMinVersion` (web `deviceCanReceiveSession`): an unreported or
    /// unparsable version = an old build. `major.minor.patch` with any
    /// `-`/`+` suffix ignored, missing segments 0 (web `parseVersionTuple`).
    public static func canReceiveSession(version: String?) -> Bool {
        guard let version, let have = parseVersionTuple(version) else { return false }
        let want = [moveMinVersion.0, moveMinVersion.1, moveMinVersion.2]
        for index in 0..<3 where have[index] != want[index] {
            return have[index] > want[index]
        }
        return true
    }

    /// `major.minor.patch` off a marketing version (web `parseVersionTuple`):
    /// the part before the first `-`/`+`, split on `.`; a segment counts only
    /// when it is ASCII digits (else 0), and no numeric major = nil.
    static func parseVersionTuple(_ version: String) -> [Int]? {
        let core = version.prefix { $0 != "-" && $0 != "+" }
        let parts = core.split(separator: ".", omittingEmptySubsequences: false)
        let number = { (index: Int) -> Int? in
            guard parts.indices.contains(index) else { return nil }
            let part = parts[index]
            guard !part.isEmpty, part.allSatisfy({ $0.isASCII && $0.isNumber }) else { return nil }
            return Int(part)
        }
        guard let major = number(0) else { return nil }
        return [major, number(1) ?? 0, number(2) ?? 0]
    }

    /// One target's refusal, or nil = movable. Order ×4 (web
    /// `deviceMoveTargetReason`): offline, visibility, update (a build before
    /// `moveMinVersion`), no-login. `agent` = the session's agent (nil skips
    /// the login check).
    public static func rowBlockedReason(
        _ device: SteerDevice,
        sessionTeamId: String,
        sessionShared: Bool,
        agent: String?
    ) -> String? {
        if !device.isOnline { return reasonOffline }
        if sessionShared != sharedWithTeam(device, teamId: sessionTeamId) { return reasonVisibility }
        if !canReceiveSession(version: device.version) { return reasonUpdate }
        if let agent, !agent.isEmpty, !device.agentIds.contains(agent) { return reasonNoLogin }
        return nil
    }

    /// One device row of the picker.
    public struct Row: Equatable, Sendable {
        public let device: SteerDevice
        /// The session runs here now (the picker's selected row).
        public let current: Bool
        public let blockedReason: String?

        public init(device: SteerDevice, current: Bool, blockedReason: String?) {
            self.device = device
            self.current = current
            self.blockedReason = blockedReason
        }

        public static func == (a: Row, b: Row) -> Bool {
            a.device.deviceId == b.device.deviceId && a.current == b.current
                && a.blockedReason == b.blockedReason
        }
    }

    /// The launch composer's devices for the session's team (own +
    /// team-shared, in its order), the current one selected and never
    /// refused, every other carrying its refusal. The current device is
    /// ALWAYS listed (web `sessionDeviceMoveRows`): when the list lacks it
    /// (a shared session on a teammate's unshared box), a bare row named
    /// by its id leads, so a pending move still has a row to cancel on.
    public static func rows(
        devices: [SteerDevice],
        currentDeviceId: String,
        sessionTeamId: String,
        sessionShared: Bool,
        agent: String?
    ) -> [Row] {
        var rows = devices.map { device in
            let current = device.deviceId == currentDeviceId
            return Row(
                device: device,
                current: current,
                blockedReason: current ? nil : rowBlockedReason(
                    device,
                    sessionTeamId: sessionTeamId,
                    sessionShared: sessionShared,
                    agent: agent
                )
            )
        }
        if !currentDeviceId.isEmpty, !rows.contains(where: \.current) {
            rows.insert(
                Row(
                    device: SteerDevice(deviceId: currentDeviceId, deviceLabel: currentDeviceId),
                    current: true,
                    blockedReason: nil
                ),
                at: 0
            )
        }
        return rows
    }

    /// The pin's text: the current device's name, or `Moving to <target>…`
    /// while `moveToDeviceId` is set (an unknown target reads as its id).
    public static func pinLabel(
        currentDeviceName: String,
        moveToDeviceId: String?,
        devices: [SteerDevice]
    ) -> String {
        guard let moveToDeviceId, !moveToDeviceId.isEmpty else { return currentDeviceName }
        let target = devices.first { $0.deviceId == moveToDeviceId }
        let name = target.map { $0.deviceLabel.isEmpty ? $0.deviceId : $0.deviceLabel } ?? moveToDeviceId
        return movingLabel(name)
    }

    /// What a pick does.
    public enum Pick: Equatable, Sendable {
        /// Request a move to that device.
        case move(String)
        /// Picking the current device while moving = cancel the move.
        case cancel
        /// Nothing to do (the current device, no move pending; or the
        /// pending target again).
        case none
    }

    public static func pick(deviceId: String, currentDeviceId: String, moveToDeviceId: String?) -> Pick {
        let moving = moveToDeviceId.map { !$0.isEmpty } ?? false
        if deviceId == currentDeviceId { return moving ? .cancel : .none }
        if moving, deviceId == moveToDeviceId { return .none }
        return .move(deviceId)
    }

    /// The wire form of a pick: a move and a cancel are the same call (a
    /// cancel names the CURRENT device). nil = nothing to send.
    public static func sendInput(sessionId: String, pick: Pick, currentDeviceId: String) -> SessionsSendInput? {
        switch pick {
        case let .move(deviceId): .moveDevice(sessionId: sessionId, deviceId: deviceId)
        case .cancel: .moveDevice(sessionId: sessionId, deviceId: currentDeviceId)
        case .none: nil
        }
    }
}
