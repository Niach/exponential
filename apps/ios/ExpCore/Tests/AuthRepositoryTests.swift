import Foundation
import XCTest
@testable import ExpCore

// Multi-account authentication gates. `isAuthenticated` describes the ACTIVE
// account only, and a per-server sign out deliberately re-adds the URL as a
// tokenless (pending) record — which becomes active. Surfaces that resolve their
// own account (the Share Extension picks the board's account) must therefore
// gate on `hasAuthenticatedAccount` / `authenticatedAccountIds`, the same rule
// the app's nav gate uses.
final class AuthRepositoryTests: XCTestCase {
    private final class FakeKeychain: KeychainStoring, @unchecked Sendable {
        private let lock = NSLock()
        private var storage: [String: String] = [:]
        func get(_ key: String) throws -> String? { lock.withLock { storage[key] } }
        @discardableResult
        func set(_ key: String, value: String?) -> Bool {
            lock.withLock { storage[key] = value }
            return true
        }
        func delete(_ key: String) { lock.withLock { storage[key] = nil } }
    }

    private let cloud = "https://app.exponential.at"
    private let selfHosted = "https://exp.example.com"

    private func makeAuth() -> AuthRepository {
        AuthRepository(accountStore: AccountStore(keychain: FakeKeychain()))
    }

    private func signIn(_ auth: AuthRepository, url: String, userId: String, token: String) {
        auth.setInstanceUrl(url)
        auth.setToken(token, email: "\(userId)@x.com", userId: userId, onboardingKnown: true)
    }

    func testNoAccountsIsNotAuthenticated() {
        let auth = makeAuth()
        XCTAssertFalse(auth.hasAuthenticatedAccount)
        XCTAssertTrue(auth.authenticatedAccountIds.isEmpty)
    }

    // Signing out of one server must not disable per-account surfaces for the
    // accounts that are still signed in.
    func testSignOutOfOneServerKeepsOtherAccountsAuthenticated() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        signIn(auth, url: selfHosted, userId: "B", token: "tB")
        let cloudId = ServerAccount.makeId(instanceUrl: cloud, userId: "A")
        let selfHostedId = ServerAccount.makeId(instanceUrl: selfHosted, userId: "B")
        XCTAssertEqual(auth.authenticatedAccountIds, [cloudId, selfHostedId])

        // The per-server sign out (ServerDetailView): drop the account, then
        // re-add the URL so the server stays listed — leaving a TOKENLESS record
        // as the active account.
        auth.removeAccount(id: selfHostedId)
        auth.setInstanceUrl(selfHosted)

        XCTAssertFalse(auth.isAuthenticated, "the active account is the tokenless re-added server")
        XCTAssertTrue(auth.hasAuthenticatedAccount, "cloud is still signed in")
        XCTAssertEqual(auth.authenticatedAccountIds, [cloudId], "only the token-holding account counts")
    }

    // The dead-session sign-out (SessionGate → SyncManager): the token goes,
    // the RECORD stays — LoginView needs the instance URL to re-authenticate
    // against, and the per-user id keeps the local cache addressable. Scoped to
    // the one rejected account.
    func testSignOutLocallyClearsOneTokenAndKeepsTheRecord() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        signIn(auth, url: selfHosted, userId: "B", token: "tB")
        let cloudId = ServerAccount.makeId(instanceUrl: cloud, userId: "A")
        let selfHostedId = ServerAccount.makeId(instanceUrl: selfHosted, userId: "B")

        auth.signOutLocally(accountId: selfHostedId)

        XCTAssertEqual(auth.accounts.count, 2, "the rejected server stays listed")
        let signedOut = auth.accounts.first { $0.id == selfHostedId }
        XCTAssertNil(signedOut?.token)
        XCTAssertEqual(signedOut?.instanceUrl, selfHosted)
        XCTAssertEqual(signedOut?.userEmail, "B@x.com", "LoginView can still name the account")
        XCTAssertEqual(auth.authenticatedAccountIds, [cloudId], "the other account keeps syncing")
    }

    // Single-account devices (the reported case) must land on the nav gate's
    // "every account is signed out" branch, which is what shows LoginView.
    func testSignOutLocallyOfTheOnlyAccountLeavesNothingAuthenticated() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")

        auth.signOutLocally(accountId: ServerAccount.makeId(instanceUrl: cloud, userId: "A"))

        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertFalse(auth.hasAuthenticatedAccount)
        XCTAssertTrue(auth.accounts.allSatisfy { $0.token == nil })
        XCTAssertEqual(auth.instanceUrl, cloud, "the login screen still knows the server")
    }

    // A re-login after the dead-session sign-out must resolve back to the SAME
    // per-user account (in-place token refresh), not strand a duplicate record.
    func testReloginAfterLocalSignOutReusesTheSameAccount() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        let cloudId = ServerAccount.makeId(instanceUrl: cloud, userId: "A")

        auth.signOutLocally(accountId: cloudId)
        auth.setToken("tA2", email: "A@x.com", userId: "A", onboardingKnown: true)

        XCTAssertEqual(auth.accounts.count, 1)
        XCTAssertEqual(auth.activeAccountId, cloudId)
        XCTAssertEqual(auth.token, "tA2")
        XCTAssertEqual(auth.authenticatedAccountIds, [cloudId])
    }

    // The reported bug: the server-side account was deleted, so signing up again
    // with the same email mints a NEW userId — the login resolves a fresh
    // per-user record and the dead-session record it came from used to stay
    // listed, showing the same server twice ("Signed out" + signed in).
    func testReloginWithANewUserIdLeavesOneRowForTheInstance() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "old", token: "tOld")
        let oldId = ServerAccount.makeId(instanceUrl: cloud, userId: "old")
        auth.signOutLocally(accountId: oldId)

        // Same instance, new server-side identity.
        auth.setToken("tNew", email: "A@x.com", userId: "new", onboardingKnown: true)
        let newId = ServerAccount.makeId(instanceUrl: cloud, userId: "new")

        XCTAssertEqual(auth.accounts.map(\.id), [newId], "the stale signed-out row is gone")
        XCTAssertEqual(auth.activeAccountId, newId)
        XCTAssertEqual(auth.token, "tNew")
        XCTAssertEqual(auth.authenticatedAccountIds, [newId])
    }

    // A signed-out row on a DIFFERENT server is not a duplicate — logging into
    // one instance must never drop another instance's re-login affordance.
    func testLoginDoesNotTouchSignedOutRowsOnOtherInstances() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        let cloudId = ServerAccount.makeId(instanceUrl: cloud, userId: "A")
        auth.signOutLocally(accountId: cloudId)

        signIn(auth, url: selfHosted, userId: "B", token: "tB")

        XCTAssertEqual(auth.accounts.count, 2)
        XCTAssertNil(auth.accounts.first { $0.id == cloudId }?.token, "still listed, still signed out")
    }

    // Self-heal for devices that already carry the duplicate (no fresh login
    // needed): SyncManager.start runs this once per launch.
    func testStartupPruneRemovesTheDuplicateSignedOutRow() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "old", token: "tOld")
        signIn(auth, url: cloud, userId: "new", token: "tNew")
        let oldId = ServerAccount.makeId(instanceUrl: cloud, userId: "old")
        let newId = ServerAccount.makeId(instanceUrl: cloud, userId: "new")
        auth.signOutLocally(accountId: oldId)
        var reclaimed: [String] = []
        auth.reclaimLocalCache = { reclaimed.append($0) }

        XCTAssertEqual(auth.pruneDuplicateSignedOutAccounts(), [oldId])

        XCTAssertEqual(auth.accounts.map(\.id), [newId])
        XCTAssertEqual(auth.activeAccountId, newId)
        XCTAssertEqual(reclaimed, [oldId], "the dropped record's local cache is reclaimed")
    }

    // Ordering guard: the cache is reclaimed BEFORE the removal is persisted. The
    // reverse orphans the account's DB files forever if the app dies mid-prune —
    // the id is gone from the store, so nothing sweeps them.
    func testPruneReclaimsTheCacheBeforePersistingTheRemoval() {
        let keychain = FakeKeychain()
        let auth = AuthRepository(accountStore: AccountStore(keychain: keychain))
        signIn(auth, url: cloud, userId: "old", token: "tOld")
        signIn(auth, url: cloud, userId: "new", token: "tNew")
        let oldId = ServerAccount.makeId(instanceUrl: cloud, userId: "old")
        auth.signOutLocally(accountId: oldId)
        var persistedAtReclaim: [String] = []
        auth.reclaimLocalCache = { _ in persistedAtReclaim.append((try? keychain.get("accounts")) ?? "") }

        XCTAssertEqual(auth.pruneDuplicateSignedOutAccounts(), [oldId])

        XCTAssertEqual(persistedAtReclaim.count, 1)
        XCTAssertTrue(
            persistedAtReclaim.first?.contains(oldId) ?? false,
            "the row is still on disk while its cache is reclaimed"
        )
        XCTAssertFalse(
            (try? keychain.get("accounts"))?.contains(oldId) ?? true,
            "and gone once the prune persists"
        )
    }

    // A lone signed-out row IS the re-login entry point — it must survive, and so
    // must a signed-out row whose only signed-in sibling is another instance.
    func testPruneKeepsSignedOutRowsWithoutASignedInSibling() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        signIn(auth, url: selfHosted, userId: "B", token: "tB")
        let cloudId = ServerAccount.makeId(instanceUrl: cloud, userId: "A")
        auth.signOutLocally(accountId: cloudId)

        XCTAssertTrue(auth.pruneDuplicateSignedOutAccounts().isEmpty)
        XCTAssertEqual(auth.accounts.count, 2)
        XCTAssertEqual(auth.instanceUrl, selfHosted, "the active account is untouched")
    }

    // Two USERS signed into one server is a supported account set, not a
    // duplicate: a token-holding row is never a prune candidate.
    func testPruneNeverRemovesASignedInRow() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        signIn(auth, url: cloud, userId: "B", token: "tB")

        XCTAssertTrue(auth.pruneDuplicateSignedOutAccounts().isEmpty)
        XCTAssertEqual(auth.accounts.count, 2)
        XCTAssertEqual(auth.authenticatedAccountIds.count, 2)
    }

    // The tokenless record an add-server / per-server sign-out flow activates is
    // the login screen's target — pruning it mid-flow would strand the user.
    func testPruneKeepsTheActiveTokenlessRecord() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        auth.setInstanceUrl(cloud)
        let pendingId = ServerAccount.makeId(for: cloud)

        XCTAssertTrue(auth.pruneDuplicateSignedOutAccounts().isEmpty)
        XCTAssertEqual(auth.activeAccountId, pendingId)
        XCTAssertEqual(auth.accounts.count, 2)
    }

    func testEveryAccountSignedOutIsNotAuthenticated() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        auth.removeAccount(id: ServerAccount.makeId(instanceUrl: cloud, userId: "A"))
        auth.setInstanceUrl(cloud)

        XCTAssertFalse(auth.hasAuthenticatedAccount)
        XCTAssertTrue(auth.authenticatedAccountIds.isEmpty)
    }
}
