import Foundation
import XCTest
@testable import ExpCore

// Per-user account identity state machine (the "logged into the wrong account"
// fix). Two users on the same server must resolve to DISTINCT account ids (hence
// distinct DB files); the same user re-logging in must refresh in place without
// a wipe; a pending (pre-login) record must re-key to the per-user id; and the
// one-shot startup migration must re-key legacy URL-keyed records.
final class AccountStoreTests: XCTestCase {
    // Mirrors AccountStore's private keychain keys (stable persisted names).
    private let keyAccounts = "accounts"
    private let keyActiveAccountId = "active_account_id"

    private final class FakeKeychain: KeychainStoring, @unchecked Sendable {
        private let lock = NSLock()
        private var storage: [String: String] = [:]
        func get(_ key: String) -> String? { lock.withLock { storage[key] } }
        func set(_ key: String, value: String?) { lock.withLock { storage[key] = value } }
        func delete(_ key: String) { lock.withLock { storage[key] = nil } }
    }

    private let url = "https://exp.example.com"

    private func resolve(_ store: AccountStore, userId: String, token: String) {
        store.resolveActiveAccount(
            token: token, email: "\(userId)@x.com", name: userId, userId: userId,
            isAdmin: false, onboardingCompletedAt: nil, onboardingKnown: true
        )
    }

    // Row 1: adding a server creates a pending (tokenless) account.
    func testAddServerCreatesPendingAccount() {
        let store = AccountStore(keychain: FakeKeychain())
        store.upsertAndActivate(instanceUrl: url)
        XCTAssertEqual(store.accounts.count, 1)
        XCTAssertEqual(store.activeAccountId, ServerAccount.makeId(for: url))
        XCTAssertNil(store.activeAccount?.token)
    }

    // Row 4 then Row 2: first login re-keys the pending record to the per-user
    // id; a second login as the same user refreshes the token in place.
    func testPendingRekeyThenSameUserRefreshInPlace() {
        let store = AccountStore(keychain: FakeKeychain())
        store.upsertAndActivate(instanceUrl: url)

        resolve(store, userId: "A", token: "t1")
        let perUserId = ServerAccount.makeId(instanceUrl: url, userId: "A")
        XCTAssertEqual(store.accounts.count, 1)
        XCTAssertEqual(store.activeAccountId, perUserId)
        XCTAssertEqual(store.activeAccount?.token, "t1")

        resolve(store, userId: "A", token: "t2")
        XCTAssertEqual(store.accounts.count, 1, "same user must not spawn a second account")
        XCTAssertEqual(store.activeAccountId, perUserId, "id must be stable → same DB file")
        XCTAssertEqual(store.activeAccount?.token, "t2")
    }

    // Row 3: signing back into an account that already exists switches to it and
    // drops the pending record we came from (no duplicate).
    func testSwitchToExistingPerUserAccountDropsPending() {
        let store = AccountStore(keychain: FakeKeychain())
        store.upsertAndActivate(instanceUrl: url)
        resolve(store, userId: "A", token: "tA")
        store.upsertAndActivate(instanceUrl: url)
        resolve(store, userId: "B", token: "tB")
        XCTAssertEqual(store.accounts.count, 2)

        // Sign back in as A: pending created, then resolve switches to the
        // existing U(A) and removes the pending.
        store.upsertAndActivate(instanceUrl: url)
        resolve(store, userId: "A", token: "tA2")
        XCTAssertEqual(store.accounts.count, 2, "pending must be dropped, not left dangling")
        XCTAssertEqual(store.activeAccountId, ServerAccount.makeId(instanceUrl: url, userId: "A"))
        XCTAssertEqual(store.activeAccount?.token, "tA2")
    }

    // Row 5: resolving a new user while another user is already the active
    // resolved account must create a fresh record, never clobber theirs.
    func testResolveNewUserOverResolvedActiveDoesNotClobber() {
        let store = AccountStore(keychain: FakeKeychain())
        store.upsertAndActivate(instanceUrl: url)
        resolve(store, userId: "A", token: "tA")

        // Directly resolve B with U(A) still active (no intervening pending).
        resolve(store, userId: "B", token: "tB")
        XCTAssertEqual(store.accounts.count, 2)
        XCTAssertEqual(store.activeAccountId, ServerAccount.makeId(instanceUrl: url, userId: "B"))
        XCTAssertEqual(store.accounts.first { $0.userId == "A" }?.token, "tA", "A must be untouched")
    }

    // Two users on the same server get distinct ids → distinct DB files.
    func testTwoUsersSameServerGetDistinctIds() {
        XCTAssertNotEqual(
            ServerAccount.makeId(instanceUrl: url, userId: "A"),
            ServerAccount.makeId(instanceUrl: url, userId: "B")
        )
    }

    // Startup migration: a legacy URL-keyed, signed-in record re-keys to its
    // per-user id (keeping its token).
    func testMigrationRekeysLegacyUrlKeyedAccount() throws {
        let fake = FakeKeychain()
        let legacyId = ServerAccount.makeId(for: url)
        let legacy = ServerAccount(
            id: legacyId, instanceUrl: url, token: "tok", userEmail: "a@x.com",
            userName: "A", userId: "A", isAdmin: false, lastUsedAt: Date()
        )
        fake.set(keyAccounts, value: String(data: try JSONEncoder().encode([legacy]), encoding: .utf8))
        fake.set(keyActiveAccountId, value: legacyId)

        let store = AccountStore(keychain: fake)
        let perUserId = ServerAccount.makeId(instanceUrl: url, userId: "A")
        XCTAssertEqual(store.accounts.count, 1)
        XCTAssertEqual(store.accounts.first?.id, perUserId)
        XCTAssertEqual(store.activeAccountId, perUserId)
        XCTAssertEqual(store.accounts.first?.token, "tok")
    }

    // Startup migration: a legacy record signed in but with no captured userId
    // can't be re-keyed — its token is nulled to force a clean re-login.
    func testMigrationNullsTokenWhenUserIdUnknown() throws {
        let fake = FakeKeychain()
        let legacyId = ServerAccount.makeId(for: url)
        let legacy = ServerAccount(
            id: legacyId, instanceUrl: url, token: "tok", userEmail: nil,
            userName: nil, userId: nil, isAdmin: false, lastUsedAt: Date()
        )
        fake.set(keyAccounts, value: String(data: try JSONEncoder().encode([legacy]), encoding: .utf8))
        fake.set(keyActiveAccountId, value: legacyId)

        let store = AccountStore(keychain: fake)
        XCTAssertEqual(store.accounts.count, 1)
        XCTAssertEqual(store.accounts.first?.id, legacyId, "no userId → id unchanged")
        XCTAssertNil(store.accounts.first?.token, "token must be cleared to force re-login")
    }
}
