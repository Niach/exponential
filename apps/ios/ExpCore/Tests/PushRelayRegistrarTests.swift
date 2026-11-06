import Foundation
import XCTest
@testable import ExpCore

// EXP-1435: the relay registration state machine: one install per relay URL,
// one capability per (relay, account), token change → PUT, 401/404 → forget
// the install and its capabilities and register again, any other failure
// throws (the app registers legacy).
final class PushRelayRegistrarTests: XCTestCase {
    private final class MemoryKeychain: KeychainStoring, @unchecked Sendable {
        var items: [String: String] = [:]
        var failWrites = false
        func get(_ key: String) throws -> String? { items[key] }
        func set(_ key: String, value: String?) -> Bool {
            if failWrites { return false }
            items[key] = value
            return true
        }
        func delete(_ key: String) { items[key] = nil }
    }

    private final class FakeRelay: PushRelayTransport, @unchecked Sendable {
        let lock = NSLock()
        var calls: [String] = []
        var installs = 0
        var caps = 0
        var updateError: PushRelayError?
        var createError: PushRelayError?

        func record(_ call: String) { lock.withLock { calls.append(call) } }

        func createInstall(relay: String, token: String, topic: String, environment: String) async throws -> PushRelayInstall {
            record("POST /installs \(token) \(topic) \(environment)")
            if let createError { throw createError }
            installs += 1
            return PushRelayInstall(installId: "ins_\(installs)", installSecret: "secret\(installs)")
        }

        func updateInstall(relay: String, install: PushRelayInstall, token: String, environment: String) async throws {
            record("PUT \(install.installId) \(token)")
            if let updateError { throw updateError }
        }

        func mintCapability(relay: String, install: PushRelayInstall, label: String) async throws -> PushRelayCapability {
            caps += 1
            record("POST caps \(install.installId) \(label)")
            return PushRelayCapability(capabilityId: "cid_\(caps)", capability: "cap_\(caps)")
        }

        func revokeCapability(relay: String, install: PushRelayInstall, capabilityId: String) async throws {
            record("DELETE \(install.installId) \(capabilityId)")
        }

        func setActivityStartToken(relay: String, install: PushRelayInstall, token: String?) async throws {
            record("PUT start \(install.installId) \(token ?? "null")")
        }

        func registerActivity(relay: String, install: PushRelayInstall, token: String) async throws -> String {
            record("POST activities \(install.installId) \(token)")
            return "act_\(token)"
        }
    }

    private func registrar(_ keychain: MemoryKeychain, _ relay: FakeRelay) -> PushRelayRegistrar {
        PushRelayRegistrar(keychain: keychain, transport: relay, topic: "at.exponential", environment: "sandbox")
    }

    func testOneInstallPerRelayAndOneCapabilityPerAccount() async throws {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        let r = registrar(keychain, relay)
        let a1 = try await r.capability(relay: "https://push.example/", accountId: "a", label: "one.example", apnsToken: "aa")
        let a2 = try await r.capability(relay: "https://push.example", accountId: "a", label: "one.example", apnsToken: "aa")
        let b = try await r.capability(relay: "https://push.example", accountId: "b", label: "two.example", apnsToken: "aa")
        XCTAssertEqual(a1.capability, "cap_1")
        XCTAssertEqual(a2.capability, "cap_1")
        XCTAssertEqual(b.capability, "cap_2")
        XCTAssertEqual(relay.calls, [
            "POST /installs aa at.exponential sandbox",
            "POST caps ins_1 one.example",
            "POST caps ins_1 two.example",
        ])

        // A new process verifies the stored install once with a PUT and
        // reuses the stored capability.
        let relay2 = FakeRelay()
        let again = try await registrar(keychain, relay2).capability(relay: "https://push.example", accountId: "a", label: "one.example", apnsToken: "aa")
        XCTAssertEqual(again.capability, "cap_1")
        XCTAssertEqual(relay2.calls, ["PUT ins_1 aa"])
    }

    func testTokenChangeUpdatesTheInstall() async throws {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        let r = registrar(keychain, relay)
        _ = try await r.capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
        _ = try await r.capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "bb")
        XCTAssertEqual(relay.calls.last, "PUT ins_1 bb")
        XCTAssertEqual(relay.installs, 1)
    }

    func testGoneInstallIsForgottenAndOtherAccountsReset() async throws {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        _ = try await registrar(keychain, relay).capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
        _ = try await registrar(keychain, relay).capability(relay: "https://push.example", accountId: "b", label: "y", apnsToken: "aa")

        relay.updateError = .gone
        let fresh = registrar(keychain, relay)
        let result = try await fresh.capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
        XCTAssertEqual(relay.installs, 2)
        XCTAssertEqual(result.capability, "cap_3")
        XCTAssertEqual(result.resetAccounts, ["b"])
    }

    func testRelayFailureThrowsAndStoresNothing() async {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        relay.createError = .status(503)
        do {
            _ = try await registrar(keychain, relay).capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
            XCTFail("expected a throw")
        } catch {
            XCTAssertEqual(error as? PushRelayError, .status(503))
        }
        XCTAssertNil(keychain.items[PushRelayRegistrar.keychainKey])
    }

    func testUnwritableKeychainThrows() async {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        keychain.failWrites = true
        do {
            _ = try await registrar(keychain, relay).capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
            XCTFail("expected a throw")
        } catch {
            XCTAssertEqual(error as? PushRelayError, .keychain)
        }
    }

    func testRevokeDeletesTheAccountsCapabilityOnly() async throws {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        let r = registrar(keychain, relay)
        _ = try await r.capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
        _ = try await r.capability(relay: "https://push.example", accountId: "b", label: "y", apnsToken: "aa")
        await r.revoke(accountId: "a")
        XCTAssertEqual(relay.calls.last, "DELETE ins_1 cid_1")
        await r.revoke(accountId: "a")
        XCTAssertEqual(relay.calls.last, "DELETE ins_1 cid_1")
        let b = try await r.capability(relay: "https://push.example", accountId: "b", label: "y", apnsToken: "aa")
        XCTAssertEqual(b.capability, "cap_2")
        let a = try await r.capability(relay: "https://push.example", accountId: "a", label: "x", apnsToken: "aa")
        XCTAssertEqual(a.capability, "cap_3")
    }

    func testRegisterInputCarriesTheRelayFields() throws {
        let sealed = try JSONSerialization.jsonObject(with: JSONEncoder().encode(
            RegisterTokenInput(token: "t", platform: "ios", capability: "cap_x", pushPublicKey: "k")
        )) as? [String: String]
        XCTAssertEqual(sealed, ["token": "t", "platform": "ios", "capability": "cap_x", "pushPublicKey": "k"])
    }

    /// #EXP-1639: the start token goes to every install, an activity token to
    /// the install holding its account's capability.
    func testLiveActivityInstallLookups() async throws {
        let keychain = MemoryKeychain(), relay = FakeRelay()
        let r = registrar(keychain, relay)
        _ = try await r.capability(relay: "https://b.example", accountId: "b", label: "b", apnsToken: "aa")
        _ = try await r.capability(relay: "https://a.example", accountId: "a", label: "a", apnsToken: "aa")
        let all = await r.installs()
        XCTAssertEqual(all.map(\.relay), ["https://a.example", "https://b.example"])
        let forB = await r.install(forAnyOf: ["x", "b"])
        XCTAssertEqual(forB?.relay, "https://b.example")
        XCTAssertEqual(forB?.install.installId, "ins_1")
        let none = await r.install(forAnyOf: ["x"])
        XCTAssertNil(none)
    }

    func testRegisterInputCarriesLiveActivitiesOnlyWhenOn() throws {
        let on = try JSONSerialization.jsonObject(with: JSONEncoder().encode(
            RegisterTokenInput(token: "t", platform: "ios", capability: "cap_x", pushPublicKey: "k", liveActivities: true)
        )) as? [String: Any]
        XCTAssertEqual(on?["liveActivities"] as? Bool, true)
        let activity = try JSONSerialization.jsonObject(with: JSONEncoder().encode(
            RegisterActivityInput(token: "t", sessionId: "s", activityId: "act_1")
        )) as? [String: String]
        XCTAssertEqual(activity, ["token": "t", "sessionId": "s", "activityId": "act_1"])
    }
}
