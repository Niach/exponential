import ExpPush
import Foundation
import XCTest

// EXP-1457: the install identity is generated once, persists, and the push
// key is derived from it (the X25519 key the registry publishes = the one the
// relay seals to).
final class InstallIdentityStoreTests: XCTestCase {
    func testIdentityIsGeneratedOnceAndPersists() throws {
        let storage = MemoryInstallSecretStorage()
        let first = try XCTUnwrap(InstallIdentityStore(storage: storage).loadOrCreate())
        XCTAssertEqual(first.count, 32)
        // A fresh store over the same keychain (= the next launch) reads it back.
        let again = InstallIdentityStore(storage: storage)
        XCTAssertEqual(again.load(), first)
        XCTAssertEqual(again.loadOrCreate(), first)
    }

    func testRotateMintsANewIdentity() throws {
        let store = InstallIdentityStore(storage: MemoryInstallSecretStorage())
        let first = try XCTUnwrap(store.loadOrCreate())
        store.rotate()
        XCTAssertNil(store.load())
        let second = try XCTUnwrap(store.loadOrCreate())
        XCTAssertNotEqual(first, second)
    }

    func testMalformedItemReadsAsAbsent() {
        let storage = MemoryInstallSecretStorage()
        _ = storage.add(account: "identity.v1", data: Data(repeating: 1, count: 31))
        XCTAssertNil(InstallIdentityStore(storage: storage).load())
    }

    /// The real keychain (the test bundle's default group): skipped where the
    /// simulator refuses keychain writes to an unsigned, hostless bundle.
    func testKeychainRoundTrip() throws {
        let storage = KeychainInstallSecretStorage(service: "at.exponential.enet.tests.\(UUID().uuidString)", group: nil)
        guard storage.add(account: "probe", data: Data([1])) else {
            throw XCTSkip("this test bundle has no keychain access")
        }
        storage.delete(account: "probe")
        let store = InstallIdentityStore(storage: storage)
        defer { store.rotate() }
        let identity = try XCTUnwrap(store.loadOrCreate())
        XCTAssertEqual(InstallIdentityStore(storage: storage).load(), identity)
    }

    #if ENET_PUSH
    func testPushKeyIsDerivedFromTheIdentity() throws {
        let identity = InstallIdentityStore(storage: MemoryInstallSecretStorage())
        let keys = PushKeyStore(storage: MemoryInstallSecretStorage(), identity: identity)
        let pair = try XCTUnwrap(keys.loadOrCreate())
        let derived = try pushKeypairDerive(identity: try XCTUnwrap(identity.load()))
        XCTAssertEqual(pair.secret, derived.secret)
        XCTAssertEqual(pair.publicKey, derived.public)
        XCTAssertEqual(keys.load(), pair)
        XCTAssertEqual(keys.loadAll(), [pair])
    }

    func testAnOlderRandomKeyStaysAsPrevious() throws {
        let storage = MemoryInstallSecretStorage()
        let legacy = try XCTUnwrap(PushCrypto.generateKeypair())
        _ = storage.add(account: "x25519.v1", data: legacy.secret + legacy.publicKey)
        let keys = PushKeyStore(storage: storage, identity: InstallIdentityStore(storage: MemoryInstallSecretStorage()))
        let derived = try XCTUnwrap(keys.loadOrCreate())
        XCTAssertNotEqual(derived, legacy)
        // The extension tries the current key, then the one it replaced.
        XCTAssertEqual(keys.loadAll(), [derived, legacy])
        // Idempotent: a second launch changes nothing.
        XCTAssertEqual(keys.loadOrCreate(), derived)
        XCTAssertEqual(keys.loadAll(), [derived, legacy])
    }
    #endif
}
