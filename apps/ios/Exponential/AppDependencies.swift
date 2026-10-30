import ExpCore
import ExpPush
import FirebaseMessaging
import Foundation
import os
import UIKit

private let logger = Logger(subsystem: "at.exponential", category: "AppDependencies")

@Observable
final class AppDependencies: @unchecked Sendable {
    let keychain: KeychainStore
    let accountStore: AccountStore
    let auth: AuthRepository
    let httpClient: HTTPClient
    let trpc: TrpcClient
    let db: DatabaseManager
    let syncManager: SyncManager
    // Kicks the shape pipelines when connectivity comes back (EXP-264) — the
    // connectivity half of the wake kick the scene phase drives.
    let networkPathWatcher: NetworkPathWatcher
    let deepLinkBus: DeepLinkBus
    // Live steer sessions outlive the screen showing them (EXP-621): their
    // sockets, feeds and composer drafts are app-scoped, so navigating away and
    // back costs no reconnect and loses no draft.
    let steerSessions = SteerSessionStore()

    // API services
    let authApi: AuthApi
    let issuesApi: IssuesApi
    let labelsApi: LabelsApi
    let teamsApi: TeamsApi
    let boardsApi: BoardsApi
    let teamMembersApi: TeamMembersApi
    let teamInvitesApi: TeamInvitesApi
    let pushTokensApi: PushTokensApi
    let integrationsApi: IntegrationsApi
    // Any-content-type issue attachments (EXP-297): upload/delete/download for
    // the Files section AND the inline-image pipeline (EXP-613 — one route).
    let attachmentsApi: AttachmentsApi
    let commentsApi: CommentsApi
    let usersApi: UsersApi
    let notificationsApi: NotificationsApi
    /// NEW-67: Settings → Voice (`voice.get` / `voice.update`).
    let voiceApi: VoiceApi
    // EXP-736: typed issue relations (blocks / parent / duplicate /
    // related) — writes go through tRPC, the rows arrive over the shape.
    let relationsApi: RelationsApi
    // EXP-878: issue drafts — rows sync (22nd shape), the three writes
    // (upsert / delete / listAttachments) are tRPC.
    let issueDraftsApi: IssueDraftsApi
    // EXP-1291: the session tree — thread roots, team agents and their
    // children sync (`sessions`, `session-issues`, `messages`); every start
    // and every message is ONE `sessions.send`, plus the stop/end/title/
    // link/answer/agent writes.
    let sessionsApi: SessionsApi
    // EXP-1291: the synced `pull_requests` rows' merge / close / files.
    let pullRequestsApi: PullRequestsApi
    let onboardingApi: OnboardingApi
    // Server-only repositories registry (not a synced shape) — read + link
    // management in team settings (masterplan §7a).
    let repositoriesApi: RepositoriesApi
    // The caller's registered machines (EXP-403) — polled, never synced.
    let devicesApi: DevicesApi
    // Team actions (EXP-253) = saved prompts — synced for reads; tRPC for
    // the prompt body and the owner-gated writes.
    let promptsApi: PromptsApi
    /// VAPP-110: the team's apps (create, rename, delete, restore, a version's package).
    let appsApi: AppsApi
    let agentsApi: AgentsApi
    // Widget/agent submission metadata (EXP-496) — tRPC-only.
    let widgetsApi: WidgetsApi

    // Push
    let pushTokenManager: PushTokenManager
    /// EXP-1457: this install's eNet node + its registry entry; nil when the
    /// build does not link eNet.
    let installNode: InstallNodeCoordinator?
    /// EXP-1459: the session-tree keys this install holds (opens sealed
    /// content, answers other readers' key requests, seals what we write).
    let treeKeyHolder: TreeKeyHolder
    let treeKeys: TreeKeyCoordinator
    let notificationDelegate: NotificationDelegate

    // EXP-452: best-effort per-account timezone claim (digest send hour).
    let timezoneClaimer: TimezoneClaimer

    init() {
        let keychain = KeychainStore()
        let accountStore = AccountStore(keychain: keychain)
        #if DEBUG
        // EXP-1267: screenshot runs start from a fresh-install state without
        // erasing the simulator (capture-only, `-uiTesting` + DEBUG).
        UITestingSession.resetIfRequested(accountStore: accountStore)
        #endif
        let auth = AuthRepository(accountStore: accountStore)
        let httpClient = HTTPClient(auth: auth)
        let trpc = TrpcClient(httpClient: httpClient, auth: auth)
        let db = DatabaseManager()
        // Teardown for records the auth layer drops on its own (the duplicate
        // signed-out server rows): same sequence as the Settings remove-server
        // path, so no DB file or picker mirror entry outlives its account.
        auth.reclaimLocalCache = { accountId in
            db.closePool(forAccountId: accountId)
            DatabaseManager.deleteFiles(forAccountId: accountId)
            SharedBoardMirror.remove(accountId: accountId)
        }
        #if DEBUG
        // EXP-1267: the session the UI test minted, persisted like a login.
        UITestingSession.injectIfProvided(into: auth)
        #endif
        // One-shot: after the keychain re-key to per-user account ids
        // (AccountStore.migratePerUserIdsIfNeeded), the legacy URL-keyed DB files
        // are orphaned — and may hold the WRONG user's cached data (the very bug
        // this fixes). Wipe them once, before any pool opens. Guarded by an
        // app-side UserDefaults flag (distinct from the keychain migration flag).
        let dbCleanupFlag = "peruser_db_cleanup_v1"
        if !UserDefaults.standard.bool(forKey: dbCleanupFlag) {
            for account in auth.accounts where account.id != ServerAccount.makeId(for: account.instanceUrl) {
                DatabaseManager.deleteFiles(forAccountId: ServerAccount.makeId(for: account.instanceUrl))
            }
            UserDefaults.standard.set(true, forKey: dbCleanupFlag)
        }
        // One-shot: after the 4→8 byte id widening (AccountStore v2 re-key ran
        // in AccountStore.init above, so auth.accounts already carry the widened
        // ids), the old short-id DB files are orphaned. Sweep every -v4 file that
        // no current account claims. Full resync of the survivors follows.
        let dbCleanupV2Flag = "peruser_db_cleanup_v2"
        if !UserDefaults.standard.bool(forKey: dbCleanupV2Flag) {
            DatabaseManager.deleteOrphanDatabaseFiles(keeping: Set(auth.accounts.map(\.id)))
            UserDefaults.standard.set(true, forKey: dbCleanupV2Flag)
        }
        // Open a pool for every signed-in account so SyncManager can launch
        // parallel shape pipelines on first tick and the UI can bind
        // ValueObservations to any account's pool without a race. The order is
        // active-first so the transitional `dbPool` getter resolves to the
        // active account during the Phase A transition.
        let activeId = auth.activeAccountId
        let orderedAccounts: [ServerAccount] = {
            var result = auth.accounts
            result.sort { a, b in
                if a.id == activeId { return true }
                if b.id == activeId { return false }
                return a.lastUsedAt > b.lastUsedAt
            }
            return result
        }()
        for account in orderedAccounts where account.token != nil {
            do {
                try db.pool(forAccountId: account.id)
            } catch {
                logger.error(
                    "Failed to open DB for account \(account.id): \(error.localizedDescription)"
                )
            }
        }
        let syncManager = SyncManager(auth: auth, db: db)
        let networkPathWatcher = NetworkPathWatcher(syncManager: syncManager)
        let deepLinkBus = DeepLinkBus()

        self.keychain = keychain
        self.accountStore = accountStore
        self.auth = auth
        self.httpClient = httpClient
        self.trpc = trpc
        self.db = db
        self.syncManager = syncManager
        self.networkPathWatcher = networkPathWatcher
        self.deepLinkBus = deepLinkBus

        // API services
        self.authApi = AuthApi(httpClient: httpClient, auth: auth)
        self.issuesApi = IssuesApi(trpc: trpc)
        self.labelsApi = LabelsApi(trpc: trpc)
        self.teamsApi = TeamsApi(trpc: trpc)
        self.boardsApi = BoardsApi(trpc: trpc)
        self.teamMembersApi = TeamMembersApi(trpc: trpc)
        self.teamInvitesApi = TeamInvitesApi(trpc: trpc)
        self.pushTokensApi = PushTokensApi(trpc: trpc)
        self.integrationsApi = IntegrationsApi(trpc: trpc)
        self.attachmentsApi = AttachmentsApi(httpClient: httpClient, trpc: trpc, auth: auth)
        self.commentsApi = CommentsApi(trpc: trpc)
        self.usersApi = UsersApi(trpc: trpc)
        self.notificationsApi = NotificationsApi(trpc: trpc)
        self.voiceApi = VoiceApi(trpc: trpc)
        self.relationsApi = RelationsApi(trpc: trpc)
        self.issueDraftsApi = IssueDraftsApi(trpc: trpc)
        self.sessionsApi = SessionsApi(trpc: trpc)
        self.pullRequestsApi = PullRequestsApi(trpc: trpc)
        self.onboardingApi = OnboardingApi(trpc: trpc)
        self.repositoriesApi = RepositoriesApi(trpc: trpc)
        self.devicesApi = DevicesApi(trpc: trpc)
        self.promptsApi = PromptsApi(trpc: trpc)
        self.appsApi = AppsApi(trpc: trpc)
        self.agentsApi = AgentsApi(trpc: trpc)
        self.widgetsApi = WidgetsApi(trpc: trpc)

        // Push notifications
        // EXP-1435: the APNs device token for the relay install comes from
        // Firebase (it receives it through its app-delegate proxy before it
        // mints the FCM token); Firebase stays for the legacy FCM path.
        let pushTokenManager = PushTokenManager(
            pushTokensApi: pushTokensApi,
            auth: auth,
            keychain: keychain,
            apnsToken: { Messaging.messaging().apnsToken }
        )
        self.pushTokenManager = pushTokenManager
        // EXP-1457 step 1: one eNet identity per install, in the shared
        // keychain group; the push key derives from it, so a rotated identity
        // re-registers the push key too.
        let identityStore = InstallIdentityStore()
        // Built by ExponentialApp's @State on the main thread.
        let (deviceLabel, devicePlatform) = MainActor.assumeIsolated {
            (UIDevice.current.name, "\(UIDevice.current.systemName) \(UIDevice.current.systemVersion)")
        }
        // EXP-1459: tree secrets in the shared keychain group (the NSE opens
        // sealed pushes with them later); the views open through `shared`.
        let treeKeyStore = TreeKeyStore()
        let keyRing = SealedKeyRing(persistence: TreeKeyPersistence(
            load: { treeKeyStore.load() },
            save: { treeKeyStore.save(rootId: $0, epoch: $1, secret: $2) },
            removeAll: { treeKeyStore.removeAll() }
        ))
        let opener = SealedOpener(crypto: SealedCryptoFactory.make(), ring: keyRing)
        SealedOpener.shared = opener
        let installNode = InstallNodeCoordinator.make(
            trpc: trpc,
            auth: auth,
            db: db,
            identity: InstallIdentitySource(
                loadOrCreate: { identityStore.loadOrCreate() },
                rotate: { identityStore.rotate() }
            ),
            label: deviceLabel,
            platform: devicePlatform,
            onIdentityRotated: {
                pushTokenManager.resetRegistrations()
                // The wraps were for the old key: fetched again for the new one.
                keyRing.removeAll()
            }
        )
        self.installNode = installNode
        let treeKeyHolder = TreeKeyHolder(
            keys: SessionKeysApi(trpc: trpc),
            installs: InstallsApi(trpc: trpc),
            crypto: opener.crypto,
            ring: keyRing,
            identity: { identityStore.load() },
            installId: { accountId in await installNode?.installId(accountId: accountId) }
        )
        self.treeKeyHolder = treeKeyHolder
        self.treeKeys = TreeKeyCoordinator(holder: treeKeyHolder, opener: opener, auth: auth, db: db)
        sessionsApi.sealing.sealer = SessionSealer(
            holder: treeKeyHolder,
            opener: opener,
            db: db,
            userId: { accountId in auth.accounts.first { $0.id == accountId }?.userId }
        )
        self.notificationDelegate = NotificationDelegate(pushTokenManager: pushTokenManager, deepLinkBus: deepLinkBus)

        self.timezoneClaimer = TimezoneClaimer(usersApi: usersApi, auth: auth)

        // Start services — SyncManager observes auth state and swaps the DB pool to
        // the active account's file before relaunching shapes, so writes never land
        // on the previous account's database.
        syncManager.start()
        // The steer sockets die in the same gaps the shapes do (EXP-625), and
        // nothing but foregrounding used to wake them, so a viewer that lost
        // the network while on screen never came back on its own. Captured, not
        // reached through `self`: this runs before init returns.
        let steerSessions = self.steerSessions
        networkPathWatcher.onRegained = { reason in
            Task { @MainActor in steerSessions.reconnectAll(reason: reason) }
        }
        // Watches for the offline → online edge and restarts the pipelines
        // there, so a recovered connection doesn't wait out a 30s backoff.
        networkPathWatcher.start()
        notificationDelegate.setup()
        // Reconcile loop: registers the FCM token for every signed-in account,
        // so logins/switches after the Messaging callback still get pushes.
        pushTokenManager.start()
        // Lazy and off the main thread: the node starts on the coordinator's
        // own task once an account is signed in.
        installNode?.start()
        treeKeys.start()
        // Claim-once loop: stamps the device timezone on accounts that never
        // had one captured (EXP-452 — mobile-only accounts digested in UTC).
        timezoneClaimer.start()
        // UI-test/screenshot runs (fastlane snapshot launches with -uiTesting)
        // must never trigger the system push-permission alert — it would sit on
        // top of every capture.
        if !ProcessInfo.processInfo.arguments.contains("-uiTesting") {
            notificationDelegate.requestPermission()
        }
    }
}
