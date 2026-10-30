import Foundation

/// EXP-1252: moving a live SESSION to another device.
///
/// A move = ONE `sessions.send({ sessionId, deviceId })` with no text
/// (`SessionsSendInput.moveDevice`): the server moves the session at once (a
/// queued row re-targets, a live one ends here and continues on the target;
/// worktree + conversation travel, logins stay). NEW-8: nothing is pending
/// on the row, so the pin always names the current device and picking it
/// does nothing.
///
/// The session composer's options line shows the Device pin BEFORE Model,
/// built exactly like the launch line's device pin. Every sentence is
/// hand-mirrored ×4 (web `lib/session-device-move.ts`, desktop
/// `crates/ui/src/device_move.rs`, Android `domain/SessionDeviceMove.kt`).
public enum SessionDeviceMove {
    /// The picker sheet's headline.
    public static let pickerTitle = "Move to device"

    /// What a move does, said once where the move is offered.
    public static let note =
        "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device."

    /// A failed request, as a toast.
    public static let failureToast = "Couldn't move the session"

    // MARK: Refusals — one sentence each, shown ON the disabled control/row

    public static let reasonSteer = "Only people who can steer this session can move it."
    public static let reasonEnded = "This session has ended."
    public static let reasonAgent = "Agents move from Settings → Agents."
    public static let reasonOffline = "This device is offline."
    public static let reasonVisibility = "This device is shared differently, so the session can't move here."
    public static let reasonNoLogin = "This device has no signed-in login for this agent."

    /// The pin exists at all: never on an agent (a persistent kind moves
    /// from Settings → Agents).
    public static func pinVisible(kind: String) -> Bool {
        !SessionKind.traits(kind).persistent
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

    /// #EXP-1440: the refusal for an own machine shared only with OTHER
    /// teams (the server's FORBIDDEN, the picker's hidden row).
    public static let deviceOtherTeamMessage = "This device belongs to another team"

    /// #EXP-1440: one of the caller's OWN machines may run a team's sessions
    /// when it is personal (shared with no team) or shared with that team; a
    /// machine shared only with other teams is theirs (web `deviceUsableInTeam`).
    public static func deviceUsableInTeam(_ device: SteerDevice, teamId: String) -> Bool {
        device.sharedTeamIds.isEmpty || sharedWithTeam(device, teamId: teamId)
    }

    /// One target's refusal, or nil = movable. Order ×4 (web
    /// `deviceMoveTargetReason`): offline, visibility, no-login. `agent` =
    /// the session's agent (nil skips the login check). Old builds are the
    /// CLIENT_MIN_VERSION floor's job, never a refusal here.
    public static func rowBlockedReason(
        _ device: SteerDevice,
        sessionTeamId: String,
        sessionShared: Bool,
        agent: String?
    ) -> String? {
        if !device.isOnline { return reasonOffline }
        if sessionShared != sharedWithTeam(device, teamId: sessionTeamId) { return reasonVisibility }
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
    /// by its id leads, so the selection always has a row.
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

    /// What a pick does.
    public enum Pick: Equatable, Sendable {
        /// Request a move to that device.
        case move(String)
        /// Nothing to do (the current device).
        case none
    }

    public static func pick(deviceId: String, currentDeviceId: String) -> Pick {
        deviceId == currentDeviceId ? .none : .move(deviceId)
    }

    /// The wire form of a pick; nil = nothing to send.
    public static func sendInput(sessionId: String, pick: Pick) -> SessionsSendInput? {
        switch pick {
        case let .move(deviceId): .moveDevice(sessionId: sessionId, deviceId: deviceId)
        case .none: nil
        }
    }
}
