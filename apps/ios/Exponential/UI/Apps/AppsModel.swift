import ExpCore
import ExpUI
import Foundation
import GRDB

/// VAPP-110: one Apps list entry, a synced app of the team's (Exponential
/// ships no built-in apps).
struct AppListEntry: Identifiable, Equatable, Sendable {
    let id: String
    let slug: String
    let name: String
    let icon: String?
    let color: String?
    let description: String
    let headVersion: Int
    let createdBy: String?
    let teamId: String?
    /// The head package's JSON text (nil = no version yet).
    let package: String?

    init(row: AppEntity) {
        id = row.id
        slug = row.slug
        name = row.name
        icon = row.icon
        color = row.color
        description = row.description
        headVersion = row.headVersion
        createdBy = row.createdBy
        teamId = row.teamId
        package = row.package
    }

    /// The pill: `v{n}`, or the `Draft` badge (muted).
    var pillLabel: String { Apps.versionLabel(headVersion) }
    var pillMuted: Bool { headVersion == 0 }

    /// The glyph (the board icon set; an unknown name falls back to the
    /// generic puzzle).
    var glyph: String {
        BoardTypeDisplay.iconName(for: icon) ?? "puzzle"
    }
}

/// The team's apps (trashed rows hidden, sorted by name)
/// and whether I own the team, live off GRDB.
@MainActor
@Observable
final class AppsListModel {
    private(set) var apps: [AppListEntry] = []
    private(set) var isOwner = false
    private(set) var loaded = false

    @ObservationIgnored private var task: Task<Void, Never>?

    nonisolated init() {}

    func bind(accountId: String, teamId: String?, userId: String?, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> ([AppEntity], Bool) in
            guard let teamId else { return ([], false) }
            let rows = try AppEntity
                .filter(Column("team_id") == teamId)
                .filter(Column("deleted_at") == nil)
                .fetchAll(db)
            let role = try String.fetchOne(
                db,
                sql: #"SELECT "role" FROM "team_members" WHERE "team_id" = ? AND "user_id" = ? LIMIT 1"#,
                arguments: [teamId, userId ?? ""]
            )
            return (rows, role == "owner")
        }
        task = Task { [weak self] in
            do {
                for try await (rows, owner) in observation.values(in: pool) {
                    guard let self else { return }
                    self.apps = Apps.sortApps(rows.map(AppListEntry.init(row:)), name: \.name)
                    self.isOwner = owner
                    self.loaded = true
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }

    /// Members rename and delete only their own apps, owners every app.
    func canManage(_ app: AppListEntry, userId: String?) -> Bool {
        isOwner || (userId != nil && app.createdBy == userId)
    }
}

/// One app's editor state, live off GRDB: the app, its
/// versions newest first, the people who wrote them, my editor session root
/// and the device a first send runs on (my default, else my first online
/// one). An app that never arrives turns `missing` after a grace period.
@MainActor
@Observable
final class AppEditorModel {
    private(set) var app: AppListEntry?
    private(set) var versions: [AppVersionEntity] = []
    private(set) var users: [UserEntity] = []
    private(set) var rootId: String?
    private(set) var rootLive = false
    private(set) var deviceId: String?
    private(set) var missing = false

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var graceTask: Task<Void, Never>?

    nonisolated init() {}

    struct Snapshot: Sendable {
        let app: AppEntity?
        let versions: [AppVersionEntity]
        let users: [UserEntity]
        let root: SessionEntity?
        let devices: [DeviceEntity]
    }

    func bind(accountId: String, appId: String, userId: String?, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let me = userId ?? ""
        let observation = ValueObservation.tracking { db -> Snapshot in
            let app = try AppEntity.fetchOne(db, key: appId)
            let versions = try AppVersionEntity
                .filter(Column("app_id") == appId)
                .order(Column("number").desc)
                .fetchAll(db)
            let authors = Set(versions.compactMap(\.createdBy))
            let users = authors.isEmpty ? [] : try UserEntity.filter(authors.contains(Column("id"))).fetchAll(db)
            let root = try SessionEntity
                .filter(Column("app_id") == appId)
                .filter(Column("user_id") == me)
                .filter(Column("parent_id") == nil)
                .fetchOne(db)
            let devices = try DeviceEntity.filter(Column("user_id") == me).fetchAll(db)
            return Snapshot(app: app, versions: versions, users: users, root: root, devices: devices)
        }
        task = Task { [weak self] in
            do {
                for try await snapshot in observation.values(in: pool) {
                    guard let self else { return }
                    if let row = snapshot.app, row.deletedAt == nil {
                        self.app = AppListEntry(row: row)
                        self.missing = false
                    } else if snapshot.app?.deletedAt != nil {
                        self.app = nil
                        self.missing = true
                    }
                    self.versions = snapshot.versions
                    self.users = snapshot.users
                    self.rootId = snapshot.root?.id
                    self.rootLive = snapshot.root?.isLive ?? false
                    self.deviceId = Self.launchDevice(snapshot.devices)
                }
            } catch {}
        }
        graceTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled, let self, self.app == nil else { return }
            self.missing = true
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        graceTask?.cancel()
        graceTask = nil
    }

    /// The device the FIRST send runs on: my default, else my first online
    /// one (web `resolveLaunchDeviceId` over my online devices).
    static func launchDevice(_ devices: [DeviceEntity], now: Date = Date()) -> String? {
        let online = devices
            .filter { DeviceLiveness.isOnline(lastSeenAt: $0.lastSeenAt, now: now) }
            .sorted { a, b in a.label.lowercased() < b.label.lowercased() }
        return (online.first(where: \.isDefault) ?? online.first)?.deviceId
    }
}
