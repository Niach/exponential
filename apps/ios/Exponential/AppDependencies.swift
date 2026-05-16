import Foundation
import GRDB

@Observable
final class AppDependencies: @unchecked Sendable {
    let keychain: KeychainStore
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

    // Push
    let pushTokenManager: PushTokenManager
    let notificationDelegate: NotificationDelegate

    init() {
        let keychain = KeychainStore()
        let auth = AuthRepository(keychain: keychain)
        let httpClient = HTTPClient(auth: auth)
        let trpc = TrpcClient(httpClient: httpClient, auth: auth)
        let db = DatabaseManager()
        let syncManager = SyncManager(auth: auth, db: db)
        let deepLinkBus = DeepLinkBus()

        self.keychain = keychain
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

        // Push notifications
        let pushTokenManager = PushTokenManager(pushTokensApi: pushTokensApi, auth: auth)
        self.pushTokenManager = pushTokenManager
        self.notificationDelegate = NotificationDelegate(pushTokenManager: pushTokenManager, deepLinkBus: deepLinkBus)

        // Start services
        syncManager.start()
        notificationDelegate.setup()
        notificationDelegate.requestPermission()
    }
}
