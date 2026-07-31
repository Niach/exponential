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
        func get(_ key: String) -> String? { lock.withLock { storage[key] } }
        func set(_ key: String, value: String?) { lock.withLock { storage[key] = value } }
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

    func testEveryAccountSignedOutIsNotAuthenticated() {
        let auth = makeAuth()
        signIn(auth, url: cloud, userId: "A", token: "tA")
        auth.removeAccount(id: ServerAccount.makeId(instanceUrl: cloud, userId: "A"))
        auth.setInstanceUrl(cloud)

        XCTAssertFalse(auth.hasAuthenticatedAccount)
        XCTAssertTrue(auth.authenticatedAccountIds.isEmpty)
    }
}
