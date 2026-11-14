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
    /// eApps V1 (#VAPP-210): the published version (0 = never published).
    let publishedVersion: Int
    let createdBy: String?
    let teamId: String?
    /// eApps V1: the PUBLISHED package's JSON text (nil = never published).
    let package: String?
    /// eApps V1: the head's package while it is ahead of the published one.
    let draftPackage: String?
    /// VAPP-120: the typed actions granted (the synced row, as stored).
    let grantedTools: [String]
    /// #EXP-1730: the app's agent (`apps.agent_id`); #FEED-239: the sidebar
    /// and Settings › Agents leave it out (`Chats.sidebarAgents`).
    let agentId: String?

    init(row: AppEntity) {
        id = row.id
        slug = row.slug
        name = row.name
        icon = row.icon
        color = row.color
        description = row.description
        headVersion = row.headVersion
        publishedVersion = row.publishedVersion
        createdBy = row.createdBy
        teamId = row.teamId
        package = row.package
        draftPackage = row.draftPackage
        grantedTools = row.grantedTools
        agentId = row.agentId
    }

    /// eApps V1: the two pointers (`Apps.shownVersion`, `listPill`, …).
    var versions: Apps.Versions {
        Apps.Versions(headVersion: headVersion, publishedVersion: publishedVersion)
    }

    /// eApps V1: the HEAD's package (the draft, else the published one).
    var headPackage: String? { draftPackage ?? package }

    /// VAPP-120: the typed actions the RUNNING package's `functions` declare
    /// (eApps V1: the published one, else the head's; `AppActions.declaredActions`,
    /// the fixture's order).
    var declaredActions: [String] {
        guard let text = package ?? draftPackage,
              let object = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any],
              let functions = object["functions"] as? [Any]
        else { return [] }
        return AppActions.declaredActions(functions.compactMap { $0 as? String })
    }

    /// FEED-159: a stored grant that is no longer grantable reads as none;
    /// round 9 §14: one stored dotted reads under its flat name.
    var grantableGranted: [String] {
        grantedTools.filter { AppActions.actionGrantable($0) }.map(AppActions.flatAction)
    }

    /// The pill = `listPill` (eApps V1: the PUBLISHED version `v{n}`, or the
    /// `Draft` badge, muted, while nothing is published).
    var pillLabel: String { Apps.listPill(versions) }
    var pillMuted: Bool { publishedVersion == 0 }

    /// The glyph (the board icon set; an unknown name falls back to the
    /// Apps glyph, `nav-apps`, ×4).
    var glyph: String {
        BoardTypeDisplay.iconName(for: icon) ?? AppIcons.navApps
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
    /// #FEED-204 (apps.json EDITOR DEVICE): the device the composer shows
    /// and sends on, its rows, and where the next send goes.
    private(set) var deviceId: String?
    private(set) var deviceRows: [DevicePickerDevice] = []
    private(set) var deviceMove = false
    /// #EXP-1729 (apps.json `editorHeader`): the shown device among the
    /// online ones (the header's `no device online` otherwise).
    private(set) var deviceOnline = false
    /// My editor root (the header's status word; a live root's Account row).
    private(set) var root: SessionEntity?
    /// #EXP-1729: Account or Model picked here (an ended root's next send
    /// then carries them).
    private(set) var picked = false
    private(set) var missing = false
    /// VAPP-177: I own the app's team (grants; delete and edit any app).
    private(set) var isOwner = false
    /// VAPP-122: the app's synced records (the Settings sheet's line).
    private(set) var recordsCount = 0
    /// eApps R3: my app access (`appAccess.mine`; nil = write until known).
    private(set) var access: String?

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var graceTask: Task<Void, Never>?
    @ObservationIgnored private var boundAppId = ""
    @ObservationIgnored private var boundUserId = ""
    @ObservationIgnored private var lastSnapshot: Snapshot?
    /// The composed devices of the last snapshot (the shown one's launch
    /// defaults, a live root's logins).
    @ObservationIgnored private var composedDevices: [SteerDevice] = []
    @ObservationIgnored private var launchState: LaunchOptionsState?

    /// #EXP-1729: the composer's Account and Model picks, seeded from the
    /// shown device's launch defaults like the launch composer's
    /// (`LaunchOptionsState`); held while the editor is open.
    var launch: LaunchOptionsState {
        if let launchState { return launchState }
        let made = LaunchOptionsState()
        launchState = made
        return made
    }

    /// eApps V1 (#VAPP-210): Publish needs write access to the app.
    var canWrite: Bool { (access ?? "write") == "write" }

    /// eApps V1: my editor session is live (its root exists, not ended), so
    /// the preview follows the head (`Apps.shownVersion`).
    var editing: Bool { rootId != nil && rootLive }

    /// eApps R3: my own level for the app (`appAccess.mine`).
    func loadAccess(accountId: String, appId: String, trpc: TrpcClient) {
        Task { [weak self] in
            let level = try? await McpServersApi(trpc: trpc).appAccessMine(accountId: accountId, appId: appId).level
            await MainActor.run { self?.access = level }
        }
    }

    /// The creator or a team owner edits, describes and deletes the app.
    func canManage(userId: String?) -> Bool {
        guard let app else { return false }
        return isOwner || (userId != nil && app.createdBy == userId)
    }

    nonisolated init() {}

    struct Snapshot: Sendable {
        let app: AppEntity?
        let versions: [AppVersionEntity]
        let users: [UserEntity]
        let root: SessionEntity?
        let devices: [DeviceEntity]
        /// The device owners' rows (a shared device's name).
        let deviceOwners: [UserEntity]
        let isOwner: Bool
        let recordsCount: Int
    }

    func bind(accountId: String, appId: String, userId: String?, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let me = userId ?? ""
        boundAppId = appId
        boundUserId = me
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
            let devices = try DeviceEntity.fetchAll(db)
            let owners = Set(devices.map(\.userId))
            let deviceOwners = try UserEntity.filter(owners.contains(Column("id"))).fetchAll(db)
            let role = try app.flatMap { row in
                try String.fetchOne(
                    db,
                    sql: #"SELECT "role" FROM "team_members" WHERE "team_id" = ? AND "user_id" = ? LIMIT 1"#,
                    arguments: [row.teamId, me]
                )
            }
            let recordsCount = try AppRecordEntity.filter(Column("app_id") == appId).fetchCount(db)
            return Snapshot(
                app: app, versions: versions, users: users, root: root, devices: devices,
                deviceOwners: deviceOwners, isOwner: role == "owner", recordsCount: recordsCount
            )
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
                    self.root = snapshot.root
                    self.lastSnapshot = snapshot
                    self.settleDevice()
                    self.isOwner = snapshot.isOwner
                    self.recordsCount = snapshot.recordsCount
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

    /// #FEED-204: the composer's "+" Device row.
    var deviceMenu: SessionComposerFooter.DeviceMenu {
        SessionComposerFooter.DeviceMenu(
            devices: deviceRows,
            current: deviceId,
            onPick: { [weak self] picked in self?.pickDevice(picked) }
        )
    }

    /// #FEED-204: an ended root's next send, elsewhere; #EXP-1729: or with
    /// the Account / Model picked here (the send carries them); nil = a
    /// plain message.
    var deviceAppMove: ChatComposerDraft.AppMove? {
        guard let deviceId, root?.status == DomainContract.sessionStatusEnded, deviceMove || picked else { return nil }
        let picks = sendPicks
        return ChatComposerDraft.AppMove(
            appId: boundAppId, deviceId: deviceId, cli: picks.cli, account: picks.account, model: picks.model
        )
    }

    /// #EXP-1729: what a send that opens or revives the root carries.
    var sendPicks: (cli: String?, account: String?, model: String?) {
        (launch.agent, AccountOptions.wireAccount(launch.account), launch.wireModel)
    }

    /// The device the composer shows, with its launch defaults and logins.
    private var shownDevice: SteerDevice? {
        guard let deviceId else { return nil }
        return composedDevices.first { $0.deviceId == deviceId }
    }

    /// #EXP-1729 (apps.json `editorHeader`): the header's `status · device`.
    var header: Apps.EditorHeader {
        Apps.editorHeader(
            root: root.map { (status: $0.status, busy: $0.busy, needsInput: $0.needsInput) },
            device: headerDeviceInput
        )
    }

    /// #EXP-1732: the header's DEVICE part (the session screen's app
    /// subtitle = the tree's word · this, in `header`'s tone).
    var headerDevice: String { Apps.editorHeaderDevice(headerDeviceInput) }

    private var headerDeviceInput: (name: String, online: Bool)? {
        deviceId.map { id in (name: deviceMenu.value ?? id, online: deviceOnline) }
    }

    /// #EXP-1729 (composer-menu.json `account`, an app editor's composer):
    /// no root or an ended one = THE launch composer's Account rows over the
    /// shown device's logins (an ended root: its own agent's only, its login
    /// checked until a pick); a pick rides the next send.
    var launchAccountMenu: SessionComposerFooter.AccountMenu {
        let device = shownDevice
        let launch = launch
        let all = launch.accountOptions(on: device)
        let options = root?.status == DomainContract.sessionStatusEnded ? all.filter { $0.agent == launch.agent } : all
        return SessionComposerFooter.AccountMenu(
            options: options,
            current: launch.selectedAccount(in: options),
            disabledKeys: [],
            onPick: { [weak self] key in
                guard let option = options.first(where: { $0.key == key }) else { return }
                launch.selectAccount(option, device: device)
                self?.picked = true
            }
        )
    }

    /// #EXP-1729: the launch composer's Model pick (no root or an ended one).
    func pickModel(_ model: String) {
        launch.model = model
        picked = true
    }

    /// #EXP-1729: a LIVE root's Account rows when its own switch offers none
    /// (its device offline, not mine to steer): the root's logins, its own
    /// checked, every row refused; nil = no login reported.
    var liveAccountFallback: SessionComposerFooter.AccountMenu? {
        guard let root, root.isLive else { return nil }
        let device = composedDevices.first { $0.deviceId == root.deviceId }
        let options = SessionAccountSwitch.options(
            accounts: device?.agentAccounts,
            usage: device?.agentUsage,
            launchDefaults: device?.launchDefaults,
            cli: root.cli,
            account: root.account
        )
        guard !options.isEmpty else { return nil }
        return SessionComposerFooter.AccountMenu(
            options: options.map(\.account),
            current: options.first(where: \.current)?.account,
            disabledKeys: Set(options.map(\.account.key)),
            onPick: { _ in }
        )
    }

    /// A pick is remembered per app on this device, never as the default.
    func pickDevice(_ picked: String) {
        UserDefaults.standard.set(picked, forKey: Self.editorDeviceKey(boundAppId))
        settleDevice()
    }

    private static func editorDeviceKey(_ appId: String) -> String {
        "exp.apps.editorDevice.\(appId)"
    }

    /// #FEED-204 (apps.json EDITOR DEVICE, `Apps.editorDevice`): the online
    /// devices of the app's team (with a root: the ones it may move to), the
    /// remembered pick, a live root on its own device until a pick moves it.
    private func settleDevice() {
        guard let snapshot = lastSnapshot else { return }
        let root = snapshot.root
        let devices = DeviceQueries.compose(
            rows: snapshot.devices,
            users: snapshot.deviceOwners,
            teamId: snapshot.app?.teamId,
            userId: boundUserId,
            now: Date()
        )
        var candidates = devices.filter(\.isOnline)
        if let root {
            let agent = SessionAccountSwitch.agent(
                cli: root.cli,
                defaultAgent: devices.first { $0.deviceId == root.deviceId }?.launchDefaults?.defaultAgent
            )
            candidates = candidates.filter { device in
                device.deviceId == root.deviceId || SessionDeviceMove.rowBlockedReason(
                    device,
                    sessionTeamId: root.teamId,
                    sessionShared: root.shared,
                    agent: agent,
                    starterId: root.userId,
                    viewerId: boundUserId
                ) == nil
            }
        }
        let choice = Apps.editorDevice(
            rootStatus: root?.status,
            rootDeviceId: root?.deviceId,
            candidates: candidates.map { (deviceId: $0.deviceId, isDefault: $0.isDefaultDevice) },
            remembered: UserDefaults.standard.string(forKey: Self.editorDeviceKey(boundAppId))
        )
        var rows = candidates.map { DevicePickerDevice($0) }
        if let shown = choice.deviceId, !rows.contains(where: { $0.id == shown }) {
            let device = devices.first { $0.deviceId == shown }
                ?? SteerDevice(deviceId: shown, deviceLabel: shown)
            rows.insert(DevicePickerDevice(device), at: 0)
        }
        deviceId = choice.deviceId
        deviceRows = rows
        deviceMove = choice.move
        deviceOnline = choice.deviceId.map { id in devices.contains { $0.deviceId == id && $0.isOnline } } ?? false
        composedDevices = devices
        // #EXP-1729: the launch composer's seeding (a different device
        // reseeds Account and Model from its launch defaults) without a
        // root; an ENDED root's rows read its own agent, login and model
        // until a pick here.
        launch.reconcile(device: shownDevice)
        if let root, root.status == DomainContract.sessionStatusEnded {
            let agent = endedRootAgent(root)
            if !picked {
                launch.agent = agent
                launch.account = root.account ?? ""
                launch.model = LaunchOptionsState.seedModel(root.model, for: agent)
            } else if launch.agent != agent {
                launch.selectAgent(agent, device: shownDevice)
            }
        }
    }

    /// The ended root's agent (its `cli`, else its device's default agent).
    private func endedRootAgent(_ root: SessionEntity) -> String {
        SessionAccountSwitch.agent(
            cli: root.cli,
            defaultAgent: composedDevices.first { $0.deviceId == root.deviceId }?.launchDefaults?.defaultAgent
        ) ?? "claude"
    }
}
