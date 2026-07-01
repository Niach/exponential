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
    let terminalDock: MacTerminalDock
    let toastCenter: MacToastCenter
    // The local device-preview runtime (build/run/embed the selected run target
    // in the dedicated Preview pane). Single active preview, retained here so it
    // survives issue navigation like the terminal dock.
    let previewController: MacPreviewController

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
        // The collapsible bottom terminal dock — shared by MacShell (renders it)
        // and the preview run terminals (which mount into it).
        let terminalDock = MainActor.assumeIsolated { MacTerminalDock() }
        self.terminalDock = terminalDock
        // The terminal runner is a singleton; point it at the shared dock once so
        // interactive runs mount there instead of a per-run window.
        MainActor.assumeIsolated { MacTerminalRunner.shared.dock = terminalDock }
        let toastCenter = MainActor.assumeIsolated { MacToastCenter() }
        self.toastCenter = toastCenter
        self.previewController = MainActor.assumeIsolated { MacPreviewController() }

        // Start sync — it observes auth state and launches one shape pipeline set
        // per signed-in account, swapping pools on account switch.
        syncManager.start()
    }
}
