import Foundation

// #EXP-2161 (packages/domain-contract/fixtures/member-space.json): a member's
// private Assistant on a device shared with their team. The pure rules the
// UI reads (TS reference packages/domain-contract/src/member-space.ts),
// replayed by MemberSpaceFixtureTests; the trust half (`pinCovers`,
// `rootMark`) lives in Sealed/InstallTrust.swift.

public enum MemberSpace {
    /// Fixture `doctorKey`: the doctor item a device must read `ok` on.
    public static let doctorKey = "member_space"
    /// Fixture `deviceIdPrefix`: a space row's device id.
    public static let deviceIdPrefix = "space-"

    /// Fixture `copy`.
    public enum Copy {
        public static let optIn = "Members' Assistants"
        public static let optInTitle = "Let members run their Assistant here?"
        public static let optInBody = "Your machine holds these members' chat keys and they can run code on it with the access of its logins. You could read their Assistant chats."
        public static let optInConfirm = "Allow"
        public static let pickTitle = "Run your Assistant on {device}?"
        public static let pickBody = "{owner} and admins of this machine can read what your Assistant does here and act as it in teams that allow it. It works only there, and a new conversation starts."
        public static let pickConfirm = "Run here"
        public static let members = "Members"
        public static let remove = "Remove"
        public static let waiting = "Waiting for approval"
        public static let noDevice = "Pick a device"
        public static let ended = "Your Assistant no longer runs on {device}. Pick a device."

        public static func pickTitle(device: String) -> String {
            pickTitle.replacingOccurrences(of: "{device}", with: device)
        }

        public static func pickBody(owner: String) -> String {
            pickBody.replacingOccurrences(of: "{owner}", with: owner)
        }
    }

    /// `optInCases`: who may move one team in or out of a device's opt-in.
    public static func mayOptIn(caller: String, allowed: Bool, kind: String?, shared: Bool, doctor: String?) -> Bool {
        if !allowed { return caller == "deviceOwner" || caller == "teamOwner" }
        return caller == "deviceOwner" && kind == "server" && shared && doctor == "ok"
    }

    /// `teamCases`: the teams a space works in, in the member's order.
    public static func spaceTeams(assistantTeamIds: [String], hostOwnerTeamIds: [String], memberTeamIds: [String]) -> [String] {
        memberTeamIds.filter { assistantTeamIds.contains($0) && hostOwnerTeamIds.contains($0) }
    }

    /// The owner's opt-in row under one shared team (web `optInRow`): off is
    /// always allowed; `detail` = why the device cannot (its doctor item's).
    public static func optInRow(checked: Bool, kind: String?, doctor: DeviceDoctor?) -> (disabled: Bool, detail: String?) {
        let item = doctor?.items.first { $0.key == doctorKey }
        let able = mayOptIn(caller: "deviceOwner", allowed: true, kind: kind, shared: true, doctor: item?.state)
        return (!able && !checked, able ? nil : (item?.detail ?? DeviceReadiness.unavailable))
    }

    public struct Device: Equatable, Sendable {
        public let rowId: String
        public let deviceId: String
        public let userId: String
        public let label: String
        public let online: Bool
        public let assistantTeamIds: [String]
        public let spaceHostId: String?

        public init(rowId: String, deviceId: String, userId: String, label: String, online: Bool, assistantTeamIds: [String], spaceHostId: String?) {
            self.rowId = rowId
            self.deviceId = deviceId
            self.userId = userId
            self.label = label
            self.online = online
            self.assistantTeamIds = assistantTeamIds
            self.spaceHostId = spaceHostId
        }
    }

    public struct SectionRow: Equatable, Sendable {
        public let id: String
        public let label: String
        public let disabled: Bool
        public let selected: Bool
    }

    public struct Section: Equatable, Sendable {
        public let teamId: String
        public let title: String
        public let rows: [SectionRow]
    }

    /// `sectionCases`: Runs on's team sections, other people's opted-in
    /// hosts per team (the member's team order, label order inside).
    public static func spaceSections(devices: [Device], teams: [(id: String, name: String)], boundId: String?, me: String) -> [Section] {
        let boundHost = devices.first { $0.deviceId == boundId }?.spaceHostId
        return teams.compactMap { team in
            let rows = devices
                .filter { $0.userId != me && $0.spaceHostId == nil && $0.assistantTeamIds.contains(team.id) }
                .sorted { $0.label < $1.label }
                .map { device in
                    let selected = boundHost != nil && device.rowId == boundHost
                    return SectionRow(id: device.deviceId, label: device.label, disabled: !device.online && !selected, selected: selected)
                }
            return rows.isEmpty ? nil : Section(teamId: team.id, title: team.name, rows: rows)
        }
    }
}

public extension DeviceEntity {
    /// #EXP-2161 (member-space.json `listed`): THE selector of every device
    /// list and picker. A member space is never a device; only what resolves
    /// a machine id a session or the Assistant names reads the raw rows.
    static func listed(_ rows: [DeviceEntity]) -> [DeviceEntity] {
        rows.filter { $0.spaceHostId == nil }
    }

    var isMemberSpace: Bool { spaceHostId != nil }
}

public extension MemberSpace {
    /// `copy.optIn*` (web `memberSpaceOptInPrompt`): Cancel + Allow.
    static func optInPrompt() -> PromptCopy {
        primaryConfirm(title: Copy.optInTitle, body: Copy.optInBody, id: "allow", label: Copy.optInConfirm)
    }

    /// `copy.pick*` (web `memberSpacePickPrompt`): Cancel + Run here.
    static func pickPrompt(device: String, owner: String) -> PromptCopy {
        primaryConfirm(title: Copy.pickTitle(device: device), body: Copy.pickBody(owner: owner), id: "pick", label: Copy.pickConfirm)
    }

    private static func primaryConfirm(title: String, body: String, id: String, label: String) -> PromptCopy {
        PromptCopy(
            title: title, body: body,
            actions: [PromptAction(id: "cancel", label: "Cancel", role: .cancel), PromptAction(id: id, label: label, role: .primary)],
            focus: id
        )
    }
}

public extension MemberSpace {
    /// The device Settings › Assistant's Account and Model rows read while it
    /// runs on a space (web `spaceDevice`): the space's own row (its logins
    /// are the host's), the host's logins while the space reports none yet.
    static func accountDevice(space: DeviceEntity, host: DeviceEntity?, me: String?, now: Date = Date()) -> SteerDevice {
        let reported = space.agentAccounts.map { !$0.isEmpty && $0 != "{}" && $0 != "null" } ?? false
        guard let host, !reported else { return SteerDevice(entity: space, now: now, currentUserId: me) }
        let merged = DeviceEntity(
            id: space.id, userId: space.userId, deviceId: space.deviceId, label: space.label, kind: space.kind,
            icon: space.icon, platform: space.platform, version: space.version, agents: host.agents, caps: space.caps,
            unauthedAgents: host.unauthedAgents, acpAgents: host.acpAgents, launchDefaults: space.launchDefaults,
            launchDefaultsUpdatedAt: space.launchDefaultsUpdatedAt, agentAccounts: host.agentAccounts,
            agentUsage: host.agentUsage, agentUsageAt: host.agentUsageAt, doctor: space.doctor, runner: space.runner,
            activeSessions: space.activeSessions, lastSeenAt: space.lastSeenAt, sharedTeamIds: [],
            spaceHostId: space.spaceHostId, spaceTeamId: space.spaceTeamId, isDefault: false,
            updateRequestedAt: nil, createdAt: space.createdAt, updatedAt: space.updatedAt
        )
        return SteerDevice(entity: merged, now: now, currentUserId: me)
    }

    /// A team device's Runs on row (web `SpacePick`): `value` names the team
    /// too (one host may be listed under several).
    struct Pick: Equatable, Sendable, Identifiable {
        public var id: String { value }
        public let value: String
        public let teamId: String
        public let title: String
        public let hostDeviceId: String
        public let ownerId: String
        public let label: String
        public let icon: String?
        public let kind: String
        public let online: Bool
        public let disabled: Bool
        public let selected: Bool
    }

    /// The picks of `spaceSections` in order, and the picked one's value
    /// (web `spacePicks`): the space's own team's row when it is listed twice.
    static func picks(rows: [DeviceEntity], teams: [(id: String, name: String)], boundId: String?, me: String, now: Date = Date()) -> (picks: [Pick], value: String?) {
        let sections = spaceSections(
            devices: rows.map { row in
                Device(
                    rowId: row.id, deviceId: row.deviceId, userId: row.userId, label: row.label,
                    online: DeviceLiveness.isOnline(lastSeenAt: row.lastSeenAt, now: now),
                    assistantTeamIds: row.assistantTeamIds, spaceHostId: row.spaceHostId
                )
            },
            teams: teams, boundId: boundId, me: me
        )
        let picks = sections.flatMap { section in
            section.rows.map { row -> Pick in
                let host = rows.first { $0.deviceId == row.id && $0.userId != me && $0.assistantTeamIds.contains(section.teamId) }
                return Pick(
                    value: "\(section.teamId):\(row.id)", teamId: section.teamId, title: section.title, hostDeviceId: row.id,
                    ownerId: host?.userId ?? "", label: row.label.isEmpty ? row.id : row.label, icon: host?.icon,
                    kind: host?.kind ?? "server",
                    online: host.map { DeviceLiveness.isOnline(lastSeenAt: $0.lastSeenAt, now: now) } ?? false,
                    disabled: row.disabled, selected: row.selected
                )
            }
        }
        let space = rows.first { $0.deviceId == boundId && $0.spaceHostId != nil }
        let picked = picks.filter(\.selected)
        let value = (picked.first { $0.teamId == space?.spaceTeamId } ?? picked.first)?.value
        return (picks, value)
    }
}
