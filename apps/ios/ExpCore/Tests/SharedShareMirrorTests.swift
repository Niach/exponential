import Foundation
import XCTest
@testable import ExpCore

// EXP-1635: what the share extension reads, and the sealing it shares with
// the app (`SendSealing`).
final class SharedShareMirrorTests: XCTestCase {
    private let account = "share-mirror-test"

    override func tearDown() {
        SharedShareMirror.remove(accountId: account)
    }

    private func device(_ id: String, online: Bool, isDefault: Bool = false, mine: Bool = true, teams: [String] = []) -> MirroredDevice {
        MirroredDevice(
            deviceId: id, rowIds: [], label: id, glyph: "laptop",
            online: online, isDefault: isDefault, isMine: mine, sharedTeamIds: teams
        )
    }

    private func targets(devices: [MirroredDevice]) -> MirroredShareTargets {
        MirroredShareTargets(userId: "u1", agents: [], chats: [], devices: devices)
    }

    func testStartDevicesLeadWithTheDefaultAndKeepOtherTeamsOut() {
        let list = targets(devices: [
            device("first", online: true),
            device("default", online: true, isDefault: true),
            device("away", online: false, isDefault: true),
            device("teammate", online: true, mine: false, teams: ["t2"]),
        ])
        XCTAssertEqual(list.startDevices(teamId: "t1").map(\.deviceId), ["default", "first", "away"])
        XCTAssertEqual(list.startDevices(teamId: "t2").map(\.deviceId), ["default", "first", "teammate", "away"])
    }

    func testRoundTripAndTeardown() {
        let written = MirroredShareTargets(
            userId: "u1",
            agents: [MirroredAgent(
                id: "a", teamId: "t", name: "Assistant", glyph: "bot", color: "#fff", sortOrder: 0,
                role: "assistant", ownerId: "u1", conversation: nil
            )],
            chats: [MirroredChat(
                title: "Chat", updatedAt: "2026-11-04T10:00:00.000Z",
                root: MirroredSealRoot(id: "r", teamId: "t", deviceId: "d", shared: false, publicKey: "p", epoch: 2)
            )],
            devices: [device("d", online: true)]
        )
        SharedShareMirror.write(accountId: account, written)
        XCTAssertEqual(SharedShareMirror.read(accountId: account), written)
        // Every sign-out path goes through the boards mirror's teardown.
        SharedBoardMirror.remove(accountId: account)
        XCTAssertNil(SharedShareMirror.read(accountId: account))
    }

    func testSealRootCarriesTheKeyOnlyWhenWhole() {
        let keyed = MirroredSealRoot(id: "r", teamId: "t", deviceId: "d", shared: true, publicKey: "p", epoch: 2)
        XCTAssertEqual(keyed.sealRoot.key, TreeSealKey(publicKey: "p", epoch: 2))
        XCTAssertNil(MirroredSealRoot(id: "r", teamId: "t", deviceId: "d", shared: true, publicKey: nil, epoch: nil).sealRoot.key)
    }
}

final class SendSealingTests: XCTestCase {
    /// Marks a sealed body with its key; everything else is unused here.
    private struct StubCrypto: SealedCrypto {
        var isAvailable = true
        func installKey(identity: Data) throws -> (secret: Data, publicKey: String) { (Data(repeating: 1, count: 32), "install-public") }
        func generateTreeKey() -> GeneratedTreeKey { GeneratedTreeKey(secret: Data(repeating: 2, count: 32), publicKey: "new-tree") }
        func wrap(installPublic: String, treeSecret: Data) throws -> String { "wrapped:\(installPublic)" }
        func unwrap(installSecret: Data, wrapped: String) throws -> Data { Data(wrapped.utf8) }
        func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String { "exp-sealed:\(treePublicKey):\(epoch):\(text)" }
        func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText { .plain(body) }
        func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String { cardJson }
        func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson { OpenedCardJson(status: .plain, epoch: nil, json: cardJson) }
    }

    private func sealing(crypto: SealedCrypto, ring: SealedKeyRing = SealedKeyRing(persistence: .memory)) -> SendSealing {
        SendSealing(
            holder: TreeKeyHolder(
                keys: TreeKeyHolderTests.FakeKeys(), installs: nil, crypto: crypto, ring: ring,
                identity: { Data(repeating: 3, count: 32) }, installId: { _ in "install-me" }
            ),
            crypto: crypto
        )
    }

    private let root = SealRoot(
        id: "r", teamId: "t", deviceId: "d", shared: false, key: TreeSealKey(publicKey: "tree", epoch: 3)
    )

    func testAKnownRootSealsToItsKeyWithNoInlineSeal() async throws {
        let prepared = try await sealing(crypto: StubCrypto()).prepare(
            accountId: "a", .agent(agentId: "g", text: "hi"), root: root, mentions: nil, deviceIds: { _ in [] }
        )
        XCTAssertEqual(prepared.input.input.text, "exp-sealed:tree:3:hi")
        XCTAssertNil(prepared.input.seal)
    }

    func testANewRootCarriesItsSealAndTheAnswerAdoptsTheKey() async throws {
        let ring = SealedKeyRing(persistence: .memory)
        let sealing = sealing(crypto: StubCrypto(), ring: ring)
        let prepared = try await sealing.prepare(
            accountId: "a", .agent(agentId: "g", text: "hi"), root: nil, mentions: nil, deviceIds: { _ in [] }
        )
        XCTAssertEqual(prepared.input.input.text, "exp-sealed:new-tree:1:hi")
        XCTAssertEqual(prepared.input.seal?.installId, "install-me")
        XCTAssertEqual(prepared.input.seal?.wraps.map(\.installId), ["install-me"])
        await sealing.completed(prepared, result: SessionsSendResult(sessionId: "new-root"))
        XCTAssertEqual(ring.keys(rootId: "new-root"), [1: Data(repeating: 2, count: 32)])
    }

    /// #EXP-2141 (sealed-content.json `dedupedSend`) through the writer's own
    /// path: prepare, then the server's answer.
    func testADedupedFirstSendNeverKeepsItsOwnKey() async throws {
        let dedupe = try XCTUnwrap(try SealedContentFixtureTests.fixture()["dedupedSend"] as? [String: Any])
        let cases = try XCTUnwrap(dedupe["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let keyed = try XCTUnwrap(testCase["keyed"] as? Bool)
            let keys = DedupedKeys(
                server: try XCTUnwrap(testCase["server"] as? String),
                wraps: [HeldTreeKey(rootId: "root", epoch: 1, wrapped: theirs)]
            )
            let ring = SealedKeyRing(persistence: .memory)
            let sealing = SendSealing(
                holder: TreeKeyHolder(
                    keys: keys, installs: nil, crypto: StubCrypto(), ring: ring,
                    identity: { Data(repeating: 3, count: 32) }, installId: { _ in "install-me" }
                ),
                crypto: StubCrypto()
            )
            var prepared = try await sealing.prepare(
                accountId: "a", .agent(agentId: "g", text: "hi"), root: nil, mentions: nil, deviceIds: { _ in [] }
            )
            XCTAssertNotNil(prepared.input.seal, name)
            if keyed { prepared.input.clientMessageId = "c1" }
            await sealing.completed(prepared, result: SessionsSendResult(sessionId: "root"))

            XCTAssertEqual(keys.asked.value, testCase["asks"] as? Bool, name)
            // Its own key, or (the tree being another send's) that one's,
            // fetched from the wrap this install has of it.
            let own = Data(repeating: 2, count: 32)
            let fetch = testCase["step"] as? String == "fetch"
            for _ in 0..<100 where fetch && ring.keys(rootId: "root").isEmpty { try await Task.sleep(for: .milliseconds(10)) }
            XCTAssertEqual(ring.keys(rootId: "root"), [1: fetch ? Data(theirs.utf8) : own], name)
            XCTAssertEqual(keys.mines.value, fetch ? [["root"]] : [], name)
        }
    }

    /// #EXP-2141: a send into a chat made elsewhere (the server names it)
    /// asks for its key at once; one whose key is here asks for nothing.
    func testASendIntoAChatMadeElsewhereAsksForItsKey() async throws {
        let keys = DedupedKeys(server: "own", wraps: [HeldTreeKey(rootId: "r", epoch: 3, wrapped: theirs)])
        let ring = SealedKeyRing(persistence: .memory)
        let sealing = SendSealing(
            holder: TreeKeyHolder(
                keys: keys, installs: nil, crypto: StubCrypto(), ring: ring,
                identity: { Data(repeating: 3, count: 32) }, installId: { _ in "install-me" }
            ),
            crypto: StubCrypto()
        )
        for _ in 0..<2 {
            let prepared = try await sealing.prepare(
                accountId: "a", .agent(agentId: "g", text: "hi"), root: root, mentions: nil, deviceIds: { _ in [] }
            )
            await sealing.completed(prepared, result: SessionsSendResult(sessionId: "r"))
            for _ in 0..<100 where ring.keys(rootId: "r").isEmpty { try await Task.sleep(for: .milliseconds(10)) }
        }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(ring.keys(rootId: "r"), [3: Data(theirs.utf8)])
        XCTAssertEqual(keys.mines.value, [["r"]])
    }

    /// A 32-byte secret (`StubCrypto.unwrap` hands a wrap back as is).
    private let theirs = String(repeating: "t", count: 32)

    private final class DedupedKeys: SessionKeysRegistry, @unchecked Sendable {
        let server: String
        let wraps: [HeldTreeKey]
        let asked = Locked(false)
        let mines = Locked<[[String]?]>([])
        init(server: String, wraps: [HeldTreeKey]) {
            self.server = server
            self.wraps = wraps
        }
        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {}
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
            mines.mutate { $0.append(rootIds) }
            return wraps
        }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { [] }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {}
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            asked.mutate { $0 = true }
            switch server {
            case "own": return rootIds.map { TreePublicKey(rootId: $0, epoch: 1, publicKey: "new-tree") }
            case "other": return rootIds.map { TreePublicKey(rootId: $0, epoch: 1, publicKey: "their-tree") }
            case "otherEpoch": return rootIds.map { TreePublicKey(rootId: $0, epoch: 2, publicKey: "their-tree") }
            case "keyless": return []
            default: throw TreeKeyError.unavailable
            }
        }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult { RequestKeyResult(state: "noHost") }
    }

    // #EDE-97 (sealed-content.json `clearTree`): no silent plaintext.
    func testNoCryptoRefusesTheSendButAClearTreeTakesThePlaintext() async throws {
        let sealing = sealing(crypto: UnavailableSealedCrypto())
        do {
            _ = try await sealing.prepare(
                accountId: "a", .agent(agentId: "g", text: "hi"), root: root, mentions: ["u1"], deviceIds: { _ in [] }
            )
            XCTFail("expected the refusal")
        } catch {
            XCTAssertEqual(error.localizedDescription, noKeyRefusal)
        }
        let clear = SealRoot(id: "r", teamId: "t", deviceId: "d", shared: false, key: nil, clearTree: true)
        let prepared = try await sealing.prepare(
            accountId: "a", .agent(agentId: "g", text: "hi"), root: clear, mentions: ["u1"], deviceIds: { _ in [] }
        )
        XCTAssertEqual(prepared.input.input.text, "hi")
        XCTAssertEqual(prepared.input.mentionUserIds, ["u1"])
    }

    func testTheIdempotencyKeyRidesBesideTheForm() throws {
        let input = SealedSendInput(input: .message(sessionId: "s", text: "x"), clientMessageId: "key")
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(input)) as? NSDictionary
        XCTAssertEqual(object, ["sessionId": "s", "text": "x", "clientMessageId": "key"] as NSDictionary)
    }
}
