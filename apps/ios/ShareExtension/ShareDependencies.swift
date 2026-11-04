import ExpPush
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
    let trpc: TrpcClient
    let issuesApi: IssuesApi
    let attachmentsApi: AttachmentsApi
    let repositoriesApi: RepositoriesApi
    /// EXP-1635: a chat send is sealed like any of the app's (`SendSealing`):
    /// the install identity and the tree secrets are in the shared keychain
    /// group; the install id reads off `installs.mine` by the derived key.
    let sealing: SendSealing

    init() {
        let keychain = KeychainStore()
        let accountStore = AccountStore(keychain: keychain)
        let auth = AuthRepository(accountStore: accountStore)
        let httpClient = HTTPClient(auth: auth)
        let trpc = TrpcClient(httpClient: httpClient, auth: auth)
        self.auth = auth
        self.trpc = trpc
        self.issuesApi = IssuesApi(trpc: trpc)
        self.attachmentsApi = AttachmentsApi(httpClient: httpClient, trpc: trpc, auth: auth)
        self.repositoriesApi = RepositoriesApi(trpc: trpc)

        let crypto = ShareSealedCrypto()
        let identity = InstallIdentityStore()
        let installs = InstallsApi(trpc: trpc)
        let treeKeys = TreeKeyStore()
        // Write only: a new root's secret lands where the app finds it.
        let ring = SealedKeyRing(persistence: TreeKeyPersistence(
            load: { [:] },
            save: { treeKeys.save(rootId: $0, epoch: $1, secret: $2) },
            removeAll: {}
        ))
        let holder = TreeKeyHolder(
            keys: SessionKeysApi(trpc: trpc),
            installs: installs,
            crypto: crypto,
            ring: ring,
            identity: { identity.load() },
            installId: { accountId in
                guard let seed = identity.load(), let key = try? crypto.installKey(identity: seed) else { return nil }
                let mine = (try? await installs.mine(accountId: accountId)) ?? []
                return mine.first { $0.x25519Public == key.publicKey && $0.revokedAt == nil }?.installId
            }
        )
        self.sealing = SendSealing(holder: holder, crypto: crypto)
    }
}
