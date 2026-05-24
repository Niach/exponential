import Foundation
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "AppDependencies")

@Observable
final class AppDependencies: @unchecked Sendable {
    let keychain: KeychainStore
    let accountStore: AccountStore
    let auth: AuthRepository
    let httpClient: HTTPClient
    let trpc: TrpcClient
    let db: DatabaseManager
    let syncManager: SyncManager
    let deepLinkBus: DeepLinkBus

    // API services
    let authApi: AuthApi
    let issuesApi: IssuesApi
    let labelsApi: LabelsApi
    let workspacesApi: WorkspacesApi
    let workspaceMembersApi: WorkspaceMembersApi
    let workspaceInvitesApi: WorkspaceInvitesApi
    let pushTokensApi: PushTokensApi
    let integrationsApi: IntegrationsApi
    let adminApi: AdminApi
    let issueImagesApi: IssueImagesApi
    let commentsApi: CommentsApi
    let agentPlanApi: AgentPlanApi

    // Push
    let pushTokenManager: PushTokenManager
    let notificationDelegate: NotificationDelegate

    init() {
        let keychain = KeychainStore()
        let accountStore = AccountStore(keychain: keychain)
        let auth = AuthRepository(accountStore: accountStore)
        let httpClient = HTTPClient(auth: auth)
        let trpc = TrpcClient(httpClient: httpClient, auth: auth)
        let db = DatabaseManager()
        // Open the active account's DB before sync starts so observers can bind on first render.
        if let activeId = auth.activeAccountId {
            do {
                try db.open(accountId: activeId)
            } catch {
                logger.error("Failed to open DB for active account: \(error.localizedDescription)")
            }
        }
        let syncManager = SyncManager(auth: auth, db: db)
        let deepLinkBus = DeepLinkBus()

        self.keychain = keychain
        self.accountStore = accountStore
        self.auth = auth
        self.httpClient = httpClient
        self.trpc = trpc
        self.db = db
        self.syncManager = syncManager
        self.deepLinkBus = deepLinkBus

        // API services
        self.authApi = AuthApi(httpClient: httpClient, auth: auth)
        self.issuesApi = IssuesApi(trpc: trpc)
        self.labelsApi = LabelsApi(trpc: trpc)
        self.workspacesApi = WorkspacesApi(trpc: trpc)
        self.workspaceMembersApi = WorkspaceMembersApi(trpc: trpc)
        self.workspaceInvitesApi = WorkspaceInvitesApi(trpc: trpc)
        self.pushTokensApi = PushTokensApi(trpc: trpc)
        self.integrationsApi = IntegrationsApi(trpc: trpc)
        self.adminApi = AdminApi(trpc: trpc)
        self.issueImagesApi = IssueImagesApi(httpClient: httpClient, auth: auth)
        self.commentsApi = CommentsApi(trpc: trpc)
        self.agentPlanApi = AgentPlanApi(trpc: trpc)

        // Push notifications
        let pushTokenManager = PushTokenManager(pushTokensApi: pushTokensApi, auth: auth)
        self.pushTokenManager = pushTokenManager
        self.notificationDelegate = NotificationDelegate(pushTokenManager: pushTokenManager, deepLinkBus: deepLinkBus)

        // Start services — SyncManager observes auth state and swaps the DB pool to
        // the active account's file before relaunching shapes, so writes never land
        // on the previous account's database.
        syncManager.start()
        notificationDelegate.setup()
        notificationDelegate.requestPermission()
    }
}
