import ExpCore
import Foundation
import os

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
    // Standalone support tickets (EXP-180 helpdesk) — polled tRPC, not synced.
    let helpdeskApi: HelpdeskApi
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
    // EXP-736: typed issue relations (blocks / parent / duplicate /
    // related) — writes go through tRPC, the rows arrive over the shape.
    let relationsApi: RelationsApi
    let onboardingApi: OnboardingApi
    // Server-only repositories registry (not a synced shape) — read + link
    // management in team settings (masterplan §7a).
    let repositoriesApi: RepositoriesApi
    // Remote start + live steer viewer (relay-backed; graceful-off when the
    // instance has no relay configured).
    let steerApi: SteerApi
    // The caller's registered machines (EXP-403) — polled, never synced.
    let devicesApi: DevicesApi
    // EXP-734: merging a run's OWN pull request (the issue-less chore PR an
    // action or chat run opened) — the session rows themselves stay synced.
    let codingSessionsApi: CodingSessionsApi
    // Team action prompts (EXP-253) — tRPC-only, view + run on mobile.
    let actionsApi: ActionsApi
    // Automations (EXP-583) — synced for reads, owner-gated tRPC for writes.
    let automationsApi: AutomationsApi
    // Widget/agent submission metadata (EXP-496) — tRPC-only.
    let widgetsApi: WidgetsApi

    // Push
    let pushTokenManager: PushTokenManager
    let notificationDelegate: NotificationDelegate

    // EXP-452: best-effort per-account timezone claim (digest send hour).
    let timezoneClaimer: TimezoneClaimer

    init() {
        let keychain = KeychainStore()
        let accountStore = AccountStore(keychain: keychain)
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
        self.helpdeskApi = HelpdeskApi(trpc: trpc)
        self.teamMembersApi = TeamMembersApi(trpc: trpc)
        self.teamInvitesApi = TeamInvitesApi(trpc: trpc)
        self.pushTokensApi = PushTokensApi(trpc: trpc)
        self.integrationsApi = IntegrationsApi(trpc: trpc)
        self.attachmentsApi = AttachmentsApi(httpClient: httpClient, trpc: trpc, auth: auth)
        self.commentsApi = CommentsApi(trpc: trpc)
        self.usersApi = UsersApi(trpc: trpc)
        self.notificationsApi = NotificationsApi(trpc: trpc)
        self.relationsApi = RelationsApi(trpc: trpc)
        self.onboardingApi = OnboardingApi(trpc: trpc)
        self.repositoriesApi = RepositoriesApi(trpc: trpc)
        self.steerApi = SteerApi(trpc: trpc)
        self.devicesApi = DevicesApi(trpc: trpc)
        self.codingSessionsApi = CodingSessionsApi(trpc: trpc)
        self.actionsApi = ActionsApi(trpc: trpc)
        self.automationsApi = AutomationsApi(trpc: trpc)
        self.widgetsApi = WidgetsApi(trpc: trpc)

        // Push notifications
        let pushTokenManager = PushTokenManager(pushTokensApi: pushTokensApi, auth: auth)
        self.pushTokenManager = pushTokenManager
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
