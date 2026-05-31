import Foundation

/// Minimal dependency graph for the Share Extension — just the auth + networking
/// stack reused (verbatim) from the main app. No DB, no SyncManager, no Firebase.
///
/// `AccountStore.init` reads the active account from the shared keychain (the
/// app promotes its accounts into the shared access group on first launch), so
/// the extension is signed in exactly when the app is.
@MainActor
final class ShareDependencies {
    let auth: AuthRepository
    let issuesApi: IssuesApi
    let issueImagesApi: IssueImagesApi

    init() {
        let keychain = KeychainStore()
        let accountStore = AccountStore(keychain: keychain)
        let auth = AuthRepository(accountStore: accountStore)
        let httpClient = HTTPClient(auth: auth)
        let trpc = TrpcClient(httpClient: httpClient, auth: auth)
        self.auth = auth
        self.issuesApi = IssuesApi(trpc: trpc)
        self.issueImagesApi = IssueImagesApi(httpClient: httpClient, auth: auth)
    }
}
