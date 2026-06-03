import ExpCore
import Foundation
import os

private let logger = Logger(subsystem: "at.exponential.mac", category: "MacAppDependencies")

/// macOS composition root. Mirrors the iOS `AppDependencies` minus the iOS-only
/// pieces (Firebase, FCM push, `NotificationDelegate`). Read-only live sync for
/// A2; CRUD/agent wiring lands in A3+.
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
    let agentPlanApi: AgentPlanApi
    let adminApi: AdminApi
    let integrationsApi: IntegrationsApi
    let agentService: MacAgentService

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
        self.agentPlanApi = AgentPlanApi(trpc: trpc)
        self.adminApi = AdminApi(trpc: trpc)
        self.integrationsApi = IntegrationsApi(trpc: trpc)
        // @State initializes this composition root on the main actor, so it's safe
        // to construct the MainActor-isolated agent service here (it starts
        // heartbeats for any already-registered workspaces).
        self.agentService = MainActor.assumeIsolated { MacAgentService(auth: auth) }

        // Start sync — it observes auth state and launches one shape pipeline set
        // per signed-in account, swapping pools on account switch.
        syncManager.start()
    }
}
