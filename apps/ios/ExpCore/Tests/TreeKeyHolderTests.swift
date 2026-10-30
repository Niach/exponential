@testable import ExpCore
import Foundation
import XCTest

// EXP-1459: the install as a holder and writer of session-tree keys, over a
// fake `sessionKeys` API and the real binding.
final class TreeKeyHolderTests: XCTestCase {
    final class FakeKeys: SessionKeysRegistry, @unchecked Sendable {
        let held = Locked<[HeldTreeKey]>([])
        let pendingPairs = Locked<[PendingTreeKey]>([])
        let wrapped = Locked<[PendingTreeKeyWrap]>([])
        let created = Locked<[CreateTreeKeyInput]>([])
        let mineCalls = Locked<[[String]?]>([])
        let remote = Locked<[TreePublicKey]>([])

        func create(accountId: String, input: CreateTreeKeyInput) async throws { created.mutate { $0.append(input) } }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
            mineCalls.mutate { $0.append(rootIds) }
            return held.value.filter { rootIds?.contains($0.rootId) ?? true }
        }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { pendingPairs.value }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws { wrapped.mutate { $0 += wraps } }
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            remote.value.filter { rootIds.contains($0.rootId) }
        }
    }

    private func crypto() throws -> SealedCrypto {
        #if ENET_NODE
        return EnetSealedCrypto()
        #else
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
        #endif
    }

    private func make(_ keys: FakeKeys, ring: SealedKeyRing, identity: Data, crypto: SealedCrypto, now: Date = Date()) -> TreeKeyHolder {
        TreeKeyHolder(
            keys: keys, installs: nil, crypto: crypto, ring: ring,
            identity: { identity }, installId: { _ in "install-me" }, now: { now }
        )
    }

    func testSyncUnwrapsHeldKeysAndAnswersPending() async throws {
        let crypto = try crypto()
        let f = try SealedContentFixtureTests.fixture()
        let tree = try SealedContentFixtureTests.hex(try XCTUnwrap(f["tree"] as? [String: Any])["secret"])
        let install = try XCTUnwrap(f["install"] as? [String: Any])
        let identity = Data((0..<32).map { UInt8($0) })
        let mine = try crypto.installKey(identity: identity)

        let keys = FakeKeys()
        keys.held.mutate { $0 = [HeldTreeKey(rootId: "root-1", epoch: 1, wrapped: try! crypto.wrap(installPublic: mine.publicKey, treeSecret: tree))] }
        keys.pendingPairs.mutate {
            $0 = [
                PendingTreeKey(rootId: "root-1", epoch: 1, publicKey: "pk", targetInstallId: "install-other", targetX25519Public: install["x25519Public"] as! String),
                // A tree this install does not hold: skipped.
                PendingTreeKey(rootId: "root-2", epoch: 1, publicKey: "pk", targetInstallId: "install-other", targetX25519Public: install["x25519Public"] as! String),
            ]
        }
        let ring = SealedKeyRing(persistence: .memory)
        let holder = make(keys, ring: ring, identity: identity, crypto: crypto)
        let ran = await holder.sync(accountId: "a")
        XCTAssertTrue(ran)
        XCTAssertEqual(ring.keys(rootId: "root-1"), [1: tree])
        let wraps = keys.wrapped.value
        XCTAssertEqual(wraps.map(\.rootId), ["root-1"])
        XCTAssertEqual(try crypto.unwrap(installSecret: try SealedContentFixtureTests.hex(install["secret"]), wrapped: wraps[0].wrapped), tree)

        // The second pass only answers pending (no full re-fetch).
        await holder.sync(accountId: "a")
        XCTAssertEqual(keys.mineCalls.value.count, 1)
    }

    func testRequestIsThrottledPerRoot() async throws {
        let crypto = try crypto()
        let keys = FakeKeys()
        let holder = make(keys, ring: SealedKeyRing(persistence: .memory), identity: Data(repeating: 7, count: 32), crypto: crypto)
        await holder.request(accountId: "a", rootId: "root-9")
        await holder.request(accountId: "a", rootId: "ROOT-9")
        XCTAssertEqual(keys.mineCalls.value, [["root-9"]])
    }

    func testNewThreadSealWrapsForThisInstallAndAdopts() async throws {
        let crypto = try crypto()
        let identity = Data(repeating: 3, count: 32)
        let mine = try crypto.installKey(identity: identity)
        let ring = SealedKeyRing(persistence: .memory)
        let holder = make(FakeKeys(), ring: ring, identity: identity, crypto: crypto)
        let (seal, key) = try await holder.newThreadSeal(accountId: "a", teamId: "t", deviceIds: [], shared: false)
        XCTAssertEqual(seal.installId, "install-me")
        XCTAssertEqual(seal.epoch, 1)
        XCTAssertEqual(seal.publicKey, key.publicKey)
        XCTAssertEqual(seal.wraps.map(\.installId), ["install-me"])
        XCTAssertEqual(try crypto.unwrap(installSecret: mine.secret, wrapped: seal.wraps[0].wrapped), key.secret)
        await holder.adopt(rootId: "new-root", key: key)
        XCTAssertEqual(ring.keys(rootId: "new-root"), [1: key.secret])
    }

    func testSealKeyPrefersKnownThenServerThenCreates() async throws {
        let crypto = try crypto()
        let keys = FakeKeys()
        let ring = SealedKeyRing(persistence: .memory)
        let holder = make(keys, ring: ring, identity: Data(repeating: 5, count: 32), crypto: crypto)
        let known = TreeSealKey(publicKey: "known", epoch: 3)
        let got1 = await holder.sealKey(accountId: "a", rootId: "r", known: known, teamId: nil, deviceIds: [], shared: false)
        XCTAssertEqual(got1, known)
        keys.remote.mutate { $0 = [TreePublicKey(rootId: "r", epoch: 2, publicKey: "remote")] }
        let got2 = await holder.sealKey(accountId: "a", rootId: "r", known: nil, teamId: nil, deviceIds: [], shared: false)
        XCTAssertEqual(got2, TreeSealKey(publicKey: "remote", epoch: 2))
        XCTAssertTrue(keys.created.value.isEmpty)
        let created = await holder.sealKey(accountId: "a", rootId: "fresh", known: nil, teamId: nil, deviceIds: [], shared: false)
        XCTAssertEqual(created?.epoch, 1)
        XCTAssertEqual(keys.created.value.map(\.rootId), ["fresh"])
        XCTAssertEqual(keys.created.value.first?.wraps.map(\.installId), ["install-me"])
        XCTAssertEqual(ring.keys(rootId: "fresh").keys.sorted(), [1])
    }

    func testReadersOwnFirstDeviceAndShared() {
        func entry(_ id: String, device: String? = nil, revoked: Bool = false) -> InstallEntry {
            InstallEntry(installId: id, userId: "u", kind: "ios", endpointId: id, x25519Public: "x-\(id)", deviceId: device, revokedAt: revoked ? "2026-10-30" : nil)
        }
        let own = [entry("b"), entry("me"), entry("gone", revoked: true)]
        let team = [entry("dev", device: "row-1"), entry("peer")]
        XCTAssertEqual(TreeReaders.resolve(own: own, team: team, selfInstallId: "me", deviceIds: ["row-1"], shared: false).map(\.installId), ["me", "b", "dev"])
        XCTAssertEqual(TreeReaders.resolve(own: own, team: team, selfInstallId: "me", deviceIds: [], shared: true).map(\.installId), ["me", "b", "dev", "peer"])
    }

    func testMentionsAndAnsweredCards() {
        XCTAssertEqual(SealedMentions.emails(in: "hi @Ann@x.at and @bob@y.io. again @ann@x.at, mail me@z.at"), ["ann@x.at", "bob@y.io"])
        let question = SealedAnswer.answeredCard(["kind": "question", "question": "Q?"], AnswerMessageInput(messageId: "m", answer: " Postgres ", decision: nil, fresh: nil, feedback: nil))
        XCTAssertEqual(question["answer"] as? String, "Postgres")
        XCTAssertEqual(question["answers"] as? [String], ["Postgres"])
        let plan = SealedAnswer.answeredCard(["kind": "plan", "plan": "p"], AnswerMessageInput(messageId: "m", answer: nil, decision: .approved, fresh: true, feedback: " ", answers: .plan(["s": [" All ", ""]])))
        XCTAssertEqual(plan["decision"] as? String, "approved")
        XCTAssertEqual(plan["fresh"] as? Bool, true)
        XCTAssertTrue(plan["feedback"] is NSNull)
        XCTAssertEqual(plan["answers"] as? [String: [String]], ["s": ["All"]])
    }

    func testSealedSendInputEncodesSealAndMentionsBesideTheForm() throws {
        let input = SealedSendInput(
            input: .message(sessionId: "s", text: "exp-sealed:v1:1:x"),
            seal: NewThreadSeal(installId: "i", publicKey: "p", epoch: 1, wraps: [TreeKeyWrap(installId: "i", wrapped: "w")]),
            mentionUserIds: ["u1"]
        )
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(input)) as? NSDictionary
        XCTAssertEqual(object, [
            "sessionId": "s", "text": "exp-sealed:v1:1:x", "mentionUserIds": ["u1"],
            "seal": ["installId": "i", "publicKey": "p", "epoch": 1, "wraps": [["installId": "i", "wrapped": "w"]]],
        ] as NSDictionary)
    }
}
