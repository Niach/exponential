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
        func unwrap(installSecret: Data, wrapped: String) throws -> Data { Data() }
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

    func testAKnownRootSealsToItsKeyWithNoInlineSeal() async {
        let prepared = await sealing(crypto: StubCrypto()).prepare(
            accountId: "a", .agent(agentId: "g", text: "hi"), root: root, mentions: nil, deviceIds: { _ in [] }
        )
        XCTAssertEqual(prepared.input.input.text, "exp-sealed:tree:3:hi")
        XCTAssertNil(prepared.input.seal)
    }

    func testANewRootCarriesItsSealAndTheAnswerAdoptsTheKey() async {
        let ring = SealedKeyRing(persistence: .memory)
        let sealing = sealing(crypto: StubCrypto(), ring: ring)
        let prepared = await sealing.prepare(
            accountId: "a", .agent(agentId: "g", text: "hi"), root: nil, mentions: nil, deviceIds: { _ in [] }
        )
        XCTAssertEqual(prepared.input.input.text, "exp-sealed:new-tree:1:hi")
        XCTAssertEqual(prepared.input.seal?.installId, "install-me")
        XCTAssertEqual(prepared.input.seal?.wraps.map(\.installId), ["install-me"])
        await sealing.completed(prepared, result: SessionsSendResult(sessionId: "new-root"))
        XCTAssertEqual(ring.keys(rootId: "new-root"), [1: Data(repeating: 2, count: 32)])
    }

    func testNoCryptoSendsThePlaintextAndKeepsTheMentions() async {
        let prepared = await sealing(crypto: UnavailableSealedCrypto()).prepare(
            accountId: "a", .agent(agentId: "g", text: "hi"), root: root, mentions: ["u1"], deviceIds: { _ in [] }
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
