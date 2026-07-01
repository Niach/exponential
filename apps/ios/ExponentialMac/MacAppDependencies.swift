import ExpCore
import Foundation
import os

private let logger = Logger(subsystem: "at.exponential.mac", category: "MacAppDependencies")

/// macOS composition root. Mirrors the iOS `AppDependencies` minus the iOS-only
/// pieces (Firebase, FCM push, `NotificationDelegate`).
@Observable
final class MacAppDependencies: @unchecked Sendable {
    let keychain: KeychainStore
    let accountStore: AccountStore
    let auth: AuthRepository
    let httpClient: HTTPClient
    let trpc: TrpcClient
    let db: DatabaseManager
    let syncManager: SyncManager

    let authApi: AuthApi
    let issuesApi: IssuesApi
    let labelsApi: LabelsApi
    let commentsApi: CommentsApi
    let workspacesApi: WorkspacesApi
    let projectsApi: ProjectsApi
    let workspaceMembersApi: WorkspaceMembersApi
    let workspaceInvitesApi: WorkspaceInvitesApi
    let issueImagesApi: IssueImagesApi
    let integrationsApi: IntegrationsApi
    let notificationsApi: NotificationsApi
    let subscriptionsApi: SubscriptionsApi
    // Server-only coding-flow procs (repositories aren't a synced shape).
    let repositoriesApi: RepositoriesApi
    let codingSessionsApi: CodingSessionsApi
    let usersApi: UsersApi
    let steerApi: SteerApi
    let terminalDock: MacTerminalDock
    let toastCenter: MacToastCenter
    // The local device-preview runtime (build/run/embed the selected run target
    // in the dedicated Preview pane). Single active preview, retained here so it
    // survives issue navigation like the terminal dock.
    let previewController: MacPreviewController
    // Persistent "Start coding" settings (claude path, repos root, branch prefix,
    // personal API key) + the native launcher (§4a). One launcher, retained here
    // so a live session's completion handler outlives issue navigation.
    let codingSettings: MacCodingSettings
    let codingLauncher: MacCodingLauncher
    // Outbound steer control socket (device presence + remote "Start on my
    // desktop"). Graceful-off when the relay isn't configured.
    let steerControl: MacSteerControlChannel

    init() {
        let keychain = KeychainStore()
        let accountStore = AccountStore(keychain: keychain)
        let auth = AuthRepository(accountStore: accountStore)
        let httpClient = HTTPClient(auth: auth)
        let trpc = TrpcClient(httpClient: httpClient, auth: auth)
        let db = DatabaseManager()

        // Pre-open a pool for every signed-in account so SyncManager can launch
        // its shape pipelines on the first tick and the UI can bind
        // ValueObservations without a race (active-first ordering).
        let activeId = auth.activeAccountId
        let ordered = auth.accounts.sorted { a, b in
            if a.id == activeId { return true }
            if b.id == activeId { return false }
            return a.lastUsedAt > b.lastUsedAt
        }
        for account in ordered where account.token != nil {
            do {
                _ = try db.pool(forAccountId: account.id)
            } catch {
                logger.error("Failed to open DB for account \(account.id): \(error.localizedDescription)")
            }
        }

        let syncManager = SyncManager(auth: auth, db: db)

        self.keychain = keychain
        self.accountStore = accountStore
        self.auth = auth
        self.httpClient = httpClient
        self.trpc = trpc
        self.db = db
        self.syncManager = syncManager
        self.authApi = AuthApi(httpClient: httpClient, auth: auth)
        self.issuesApi = IssuesApi(trpc: trpc)
        self.labelsApi = LabelsApi(trpc: trpc)
        self.commentsApi = CommentsApi(trpc: trpc)
        self.workspacesApi = WorkspacesApi(trpc: trpc)
        self.projectsApi = ProjectsApi(trpc: trpc)
        self.workspaceMembersApi = WorkspaceMembersApi(trpc: trpc)
        self.workspaceInvitesApi = WorkspaceInvitesApi(trpc: trpc)
        self.issueImagesApi = IssueImagesApi(httpClient: httpClient, auth: auth)
        let integrationsApi = IntegrationsApi(trpc: trpc)
        self.integrationsApi = integrationsApi
        self.notificationsApi = NotificationsApi(trpc: trpc)
        self.subscriptionsApi = SubscriptionsApi(trpc: trpc)
        let repositoriesApi = RepositoriesApi(trpc: trpc)
        self.repositoriesApi = repositoriesApi
        let codingSessionsApi = CodingSessionsApi(trpc: trpc)
        self.codingSessionsApi = codingSessionsApi
        self.usersApi = UsersApi(trpc: trpc)
        let steerApi = SteerApi(trpc: trpc)
        self.steerApi = steerApi
        // The collapsible bottom terminal dock — shared by MacShell (renders it)
        // and the preview + coding run terminals (which mount into it).
        let terminalDock = MainActor.assumeIsolated { MacTerminalDock() }
        self.terminalDock = terminalDock
        // The terminal runner is a singleton; point it at the shared dock once so
        // interactive runs mount there instead of a per-run window.
        MainActor.assumeIsolated { MacTerminalRunner.shared.dock = terminalDock }
        let toastCenter = MainActor.assumeIsolated { MacToastCenter() }
        self.toastCenter = toastCenter
        self.previewController = MainActor.assumeIsolated { MacPreviewController() }
        let codingSettings = MainActor.assumeIsolated { MacCodingSettings.load() }
        self.codingSettings = codingSettings
        let codingLauncher = MainActor.assumeIsolated {
            MacCodingLauncher(
                auth: auth,
                repositoriesApi: repositoriesApi,
                codingSessionsApi: codingSessionsApi,
                steerApi: steerApi,
                db: db,
                settings: codingSettings,
                toasts: toastCenter,
                terminalDock: terminalDock
            )
        }
        self.codingLauncher = codingLauncher
        // The control socket routes a remote `start_session` to the same launcher
        // the local play button uses.
        let steerControl = MainActor.assumeIsolated {
            MacSteerControlChannel(auth: auth, steerApi: steerApi, settings: codingSettings) { accountId, issueId in
                codingLauncher.start(accountId: accountId, issueId: issueId)
            }
        }
        self.steerControl = steerControl

        // Start sync — it observes auth state and launches one shape pipeline set
        // per signed-in account, swapping pools on account switch.
        syncManager.start()
        // Start the steer control socket (self-gates when the relay is unset).
        MainActor.assumeIsolated { steerControl.start() }
    }
}
