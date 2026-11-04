import Foundation

// EXP-1635: what the Share Extension needs beyond the boards to start a chat,
// mirrored from the app's local DB into the app-group container (the database
// itself is per account and not shared): the agents, the recent chats, the
// devices, and each root's seal key so a share is sealed like any send.

/// A root a share lands in: what `SendSealing` reads off the local row.
public struct MirroredSealRoot: Codable, Sendable, Equatable {
    public let id: String
    public let teamId: String
    public let deviceId: String
    public let shared: Bool
    public let publicKey: String?
    public let epoch: UInt32?

    public init(id: String, teamId: String, deviceId: String, shared: Bool, publicKey: String?, epoch: UInt32?) {
        self.id = id
        self.teamId = teamId
        self.deviceId = deviceId
        self.shared = shared
        self.publicKey = publicKey
        self.epoch = epoch
    }

    var sealRoot: SealRoot {
        SealRoot(
            id: id, teamId: teamId, deviceId: deviceId, shared: shared,
            key: publicKey.flatMap { key in epoch.map { TreeSealKey(publicKey: key, epoch: $0) } }
        )
    }
}

public struct MirroredAgent: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let teamId: String
    public let name: String
    /// The resolved registry glyph and hex colour (the extension compiles
    /// neither the fallbacks nor the session tree).
    public let glyph: String
    public let color: String
    public let sortOrder: Double
    public let role: String?
    public let ownerId: String?
    /// The viewer's conversation with this agent; nil = none yet (a send
    /// creates it).
    public let conversation: MirroredSealRoot?

    public init(
        id: String, teamId: String, name: String, glyph: String, color: String, sortOrder: Double,
        role: String?, ownerId: String?, conversation: MirroredSealRoot?
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.glyph = glyph
        self.color = color
        self.sortOrder = sortOrder
        self.role = role
        self.ownerId = ownerId
        self.conversation = conversation
    }
}

extension MirroredAgent: AgentRoleOrdered {
    public var agentId: String { id }
    public var agentName: String { name }
    public var agentRole: String? { role }
    public var agentOwnerId: String? { ownerId }
    public var agentSortOrder: Double { sortOrder }
}

public struct MirroredChat: Codable, Sendable, Equatable, Identifiable {
    public let title: String
    public let updatedAt: String
    public let root: MirroredSealRoot

    public var id: String { root.id }

    public init(title: String, updatedAt: String, root: MirroredSealRoot) {
        self.title = title
        self.updatedAt = updatedAt
        self.root = root
    }
}

public struct MirroredDevice: Codable, Sendable, Equatable, Identifiable {
    /// The steer device id `sessions.send` names.
    public let deviceId: String
    /// The `devices` row ids an install's `deviceId` may name.
    public let rowIds: [String]
    public let label: String
    public let glyph: String
    /// As of the mirror's last write: an ordering hint, never a gate.
    public let online: Bool
    public let isDefault: Bool
    /// The caller's own machine; otherwise only for `sharedTeamIds`.
    public let isMine: Bool
    public let sharedTeamIds: [String]

    public var id: String { deviceId }

    public init(
        deviceId: String, rowIds: [String], label: String, glyph: String,
        online: Bool, isDefault: Bool, isMine: Bool, sharedTeamIds: [String]
    ) {
        self.deviceId = deviceId
        self.rowIds = rowIds
        self.label = label
        self.glyph = glyph
        self.online = online
        self.isDefault = isDefault
        self.isMine = isMine
        self.sharedTeamIds = sharedTeamIds
    }
}

/// One account's share destinations.
public struct MirroredShareTargets: Codable, Sendable, Equatable {
    public let userId: String?
    public let agents: [MirroredAgent]
    public let chats: [MirroredChat]
    public let devices: [MirroredDevice]

    public init(userId: String?, agents: [MirroredAgent], chats: [MirroredChat], devices: [MirroredDevice]) {
        self.userId = userId
        self.agents = agents
        self.chats = chats
        self.devices = devices
    }

    /// The machines a Temporary session of `teamId` can start on, the
    /// composer's default first (EXP-622: the caller's default machine, else
    /// the first candidate; online ones ahead).
    public func startDevices(teamId: String) -> [MirroredDevice] {
        devices
            .filter { $0.isMine || $0.sharedTeamIds.contains(teamId) }
            .enumerated()
            .sorted { lhs, rhs in
                if lhs.element.online != rhs.element.online { return lhs.element.online }
                if lhs.element.isDefault != rhs.element.isDefault { return lhs.element.isDefault }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
    }
}

/// Reads/writes the per-account share mirror in the shared app-group
/// defaults. The app writes (`ShareTargetsPublisher`); the extension reads.
public enum SharedShareMirror {
    private static func key(_ accountId: String) -> String { "share_targets_v1.\(accountId)" }

    public static func write(accountId: String, _ targets: MirroredShareTargets) {
        guard !accountId.isEmpty, let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(targets),
              defaults.data(forKey: key(accountId)) != data
        else { return }
        defaults.set(data, forKey: key(accountId))
    }

    public static func read(accountId: String) -> MirroredShareTargets? {
        guard let data = SharedAppGroup.defaults?.data(forKey: key(accountId)) else { return nil }
        return try? JSONDecoder().decode(MirroredShareTargets.self, from: data)
    }

    /// Sign-out / remove-server (`SharedBoardMirror.remove`).
    public static func remove(accountId: String) {
        guard !accountId.isEmpty else { return }
        SharedAppGroup.defaults?.removeObject(forKey: key(accountId))
    }
}
