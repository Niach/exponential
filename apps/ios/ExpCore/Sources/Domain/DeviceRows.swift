import Foundation
import GRDB

// EXP-481: the synced `devices` / `device_worktrees` rows (shapes 17/18) as
// UI-facing values. The devices row is SERVER-AUTHORITATIVE machine state;
// online-ness is a pure client-side derivation from `last_seen_at` freshness
// (devices heartbeat ~30s), so the mapping here replaces the old
// `devices.list` poll as the machines/pickers source.

// MARK: - DeviceLiveness

public enum DeviceLiveness {
    /// Whether a devices row reads "online": `last_seen_at` within the
    /// contract window of `now`. FAIL-CLOSED, unlike session liveness — an
    /// unparseable timestamp claims an unstartable machine is startable, so
    /// nil parses read offline. A negative age (client clock behind the
    /// server stamp) clamps to online.
    public static func isOnline(lastSeenAt: String?, now: Date = Date()) -> Bool {
        guard let lastSeenAt, let seen = WireTimestamps.parse(lastSeenAt) else {
            return false
        }
        let age = now.timeIntervalSince(seen)
        if age < 0 { return true }
        return age * 1000 < Double(DomainContract.deviceOnlineWindowMs)
    }
}

// MARK: - WorktreeResume

public enum WorktreeResume {
    /// The synced worktree row that makes "Resume previous session" offerable
    /// for (device, issue, agent): same device row, `issue_identifier` equal
    /// (case-insensitive — identifiers are server-uppercased but device
    /// branch parsing may differ), and the .exp-agents marker either absent
    /// (nil `agents` = any agent may resume) or containing the chosen agent.
    public static func match(
        worktrees: [DeviceWorktreeEntity],
        deviceRowId: String?,
        issueIdentifier: String?,
        agent: String
    ) -> DeviceWorktreeEntity? {
        guard let deviceRowId, let issueIdentifier else { return nil }
        return worktrees.first { row in
            guard row.deviceRowId == deviceRowId,
                  let rowIdentifier = row.issueIdentifier,
                  rowIdentifier.caseInsensitiveCompare(issueIdentifier) == .orderedSame
            else { return false }
            guard let agents = row.agentIds else { return true }
            return agents.contains(agent)
        }
    }
}

public extension DeviceWorktreeEntity {
    /// The .exp-agents marker as ids; nil = pre-marker worktree (any agent).
    var agentIds: [String]? {
        guard let agents else { return nil }
        return try? JSONDecoder().decode([String].self, from: Data(agents.utf8))
    }
}

// MARK: - SteerDevice mapping

public extension SteerDevice {
    /// A synced devices row as the `SteerDevice` every machines/picker
    /// surface renders — the sync-fed replacement for a `devices.list` entry.
    /// `owner` attribution mirrors the tRPC shape: set ONLY when the row is
    /// not the caller's own (name from the synced users shape — a sharing
    /// owner is always a teammate, hence inside it; fall back to the id).
    init(entity: DeviceEntity, now: Date = Date(), currentUserId: String?, ownerName: String? = nil) {
        let mine = currentUserId != nil && entity.userId == currentUserId
        self.init(
            deviceId: entity.deviceId,
            deviceLabel: entity.label,
            agents: Self.decodeStringArray(entity.agents),
            unauthedAgents: Self.decodeStringArray(entity.unauthedAgents) ?? [],
            caps: Self.decodeStringArray(entity.caps) ?? [],
            kind: entity.kind,
            platform: entity.platform,
            online: DeviceLiveness.isOnline(lastSeenAt: entity.lastSeenAt, now: now),
            lastSeenAt: entity.lastSeenAt,
            registered: true,
            version: entity.version,
            updateRequested: entity.updateRequestedAt != nil,
            updateBlocked: entity.updateRequestedAt != nil && entity.activeSessions > 0,
            sharedTeamId: entity.sharedTeamId,
            owner: mine ? nil : DeviceOwner(id: entity.userId, name: ownerName ?? entity.userId),
            // EXP-622: a default belongs to the row's OWNER — never surface a
            // teammate's shared server as the caller's default.
            isDefault: mine && entity.isDefault,
            launchDefaults: Self.decodeLaunchDefaults(entity.launchDefaults),
            rowId: entity.id
        )
    }

    private static func decodeStringArray(_ json: String?) -> [String]? {
        guard let json else { return nil }
        return try? JSONDecoder().decode([String].self, from: Data(json.utf8))
    }

    private static func decodeLaunchDefaults(_ json: String?) -> DeviceLaunchDefaults? {
        guard let json else { return nil }
        return try? JSONDecoder().decode(DeviceLaunchDefaults.self, from: Data(json.utf8))
    }
}

// MARK: - DeviceQueries

/// One-shot reads over the synced device rows — the sync-fed replacement for
/// the old `devices.list` start-target poll (same output shape, no network).
/// Own rows always; teammates' shared server rows only when scoped to their
/// team.
public enum DeviceQueries {
    public static func devices(
        db: DatabaseManager,
        accountId: String,
        teamId: String?,
        userId: String?,
        now: Date = Date()
    ) async -> [SteerDevice] {
        guard let pool = try? db.pool(forAccountId: accountId) else { return [] }
        let rows = (try? await pool.read { db in try DeviceEntity.fetchAll(db) }) ?? []
        let users = (try? await pool.read { db in try UserEntity.fetchAll(db) }) ?? []
        return compose(rows: rows, users: users, teamId: teamId, userId: userId, now: now)
    }

    /// The machines a remote start can reach right now — pre-filtered to
    /// online, like the tRPC helper it replaces (pickers additionally drop
    /// nothing-runnable machines themselves, EXP-409).
    public static func onlineStartTargets(
        db: DatabaseManager,
        accountId: String,
        teamId: String?,
        userId: String?,
        now: Date = Date()
    ) async -> [SteerDevice] {
        await devices(db: db, accountId: accountId, teamId: teamId, userId: userId, now: now)
            .filter(\.isOnline)
    }

    /// Pure composition (unit-testable): own rows first, then teammates' rows
    /// shared with [teamId] — the `devices.list` grouping the surfaces
    /// already render. Within each group online machines lead, sorted by
    /// label so heartbeats can't reorder them (EXP-623); offline rows don't
    /// beat, so last-seen desc is stable there.
    public static func compose(
        rows: [DeviceEntity],
        users: [UserEntity],
        teamId: String?,
        userId: String?,
        now: Date = Date()
    ) -> [SteerDevice] {
        // users.name is nullable — flatten so the lookup yields String?.
        let nameById: [String: String] = Dictionary(
            users.compactMap { user in user.name.map { (user.id, $0) } },
            uniquingKeysWith: { a, _ in a }
        )
        func mapped(_ entity: DeviceEntity) -> SteerDevice {
            SteerDevice(
                entity: entity, now: now, currentUserId: userId,
                ownerName: nameById[entity.userId]
            )
        }
        func stableOrder(_ a: DeviceEntity, _ b: DeviceEntity) -> Bool {
            let aOnline = DeviceLiveness.isOnline(lastSeenAt: a.lastSeenAt, now: now)
            let bOnline = DeviceLiveness.isOnline(lastSeenAt: b.lastSeenAt, now: now)
            if aOnline != bOnline { return aOnline }
            if !aOnline, a.lastSeenAt != b.lastSeenAt {
                // ISO stamps order lexicographically — desc = most recent first.
                return (a.lastSeenAt ?? "") > (b.lastSeenAt ?? "")
            }
            let aLabel = a.label.lowercased()
            let bLabel = b.label.lowercased()
            if aLabel != bLabel { return aLabel < bLabel }
            return a.deviceId < b.deviceId
        }
        let own = rows
            .filter { userId != nil && $0.userId == userId }
            .sorted(by: stableOrder)
        let shared = rows
            .filter { row in
                guard row.userId != userId, let teamId else { return false }
                return row.sharedTeamId == teamId
            }
            .sorted(by: stableOrder)
        return own.map(mapped) + shared.map(mapped)
    }

    /// One-shot worktree inventory read (the Start-coding sheet's resume
    /// probe and the device-settings worktree list).
    public static func worktrees(db: DatabaseManager, accountId: String) async -> [DeviceWorktreeEntity] {
        guard let pool = try? db.pool(forAccountId: accountId) else { return [] }
        return (try? await pool.read { db in try DeviceWorktreeEntity.fetchAll(db) }) ?? []
    }
}
