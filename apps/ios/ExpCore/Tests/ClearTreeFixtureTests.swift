@testable import ExpCore
import Foundation
import XCTest

// #EDE-97: packages/domain-contract/fixtures/sealed-content.json `clearTree`
// `writerCases` replayed through the pure `sendWire` AND through
// `SendSealing.prepare` over a fake server: no silent plaintext, only a
// clear tree takes the text in clear.
final class ClearTreeFixtureTests: XCTestCase {
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

    /// `reader` = a create is accepted; `keyFrom` = the publicKeys call from
    /// which on the device's key is there (nil = never).
    private final class Server: SessionKeysRegistry, @unchecked Sendable {
        let reader: Bool
        let keyFrom: Int?
        let asks = Locked<[String]>([])
        let keyCalls = Locked(0)
        init(reader: Bool, keyFrom: Int?) {
            self.reader = reader
            self.keyFrom = keyFrom
        }

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            asks.mutate { $0.append("create") }
            if !reader { throw TrpcError.httpError(403, #"{"error":{"message":"no","data":{"code":"FORBIDDEN"}}}"#) }
        }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] { [] }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { [] }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {}
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            keyCalls.mutate { $0 += 1 }
            guard let keyFrom, keyCalls.value >= keyFrom else { return [] }
            return rootIds.map { TreePublicKey(rootId: $0, epoch: 1, publicKey: "device-tree") }
        }
    }

    private static func clearTree() throws -> [String: Any] {
        try XCTUnwrap(try SealedContentFixtureTests.fixture()["clearTree"] as? [String: Any])
    }

    /// `holder`: `ready` = registered, `late` = registered at the second ask,
    /// `none` = never.
    private func sealing(_ server: Server, holder: String) -> SendSealing {
        let crypto = StubCrypto()
        let asked = Locked(0)
        return SendSealing(
            holder: TreeKeyHolder(
                keys: server, installs: nil, crypto: crypto, ring: SealedKeyRing(persistence: .memory),
                identity: { Data(repeating: 3, count: 32) },
                installId: { _ in
                    asked.mutate { $0 += 1 }
                    switch holder {
                    case "ready": return "install-me"
                    case "late": return asked.value >= 2 ? "install-me" : nil
                    default: return nil
                    }
                }
            ),
            crypto: crypto,
            holderWait: .milliseconds(1500),
            keyWait: .milliseconds(1500)
        )
    }

    func testTheCopyAndWaitsAreTheFixtures() throws {
        let f = try Self.clearTree()
        XCTAssertEqual(f["noKeyRefusal"] as? String, noKeyRefusal)
        XCTAssertEqual(f["clearRefusal"] as? String, clearTreeRefusal)
        XCTAssertEqual(f["holderWaitMs"] as? Int, 20_000)
        XCTAssertEqual(sealHolderWait, .milliseconds(try XCTUnwrap(f["holderWaitMs"] as? Int)))
        XCTAssertEqual(sealKeyWait, .milliseconds(try XCTUnwrap(f["keyWaitMs"] as? Int)))
        XCTAssertEqual(SealRefusal().localizedDescription, noKeyRefusal)
    }

    func testWriterCasesPure() throws {
        let cases = try XCTUnwrap(try Self.clearTree()["writerCases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 12)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let wire = sendWire(
                tree: try XCTUnwrap(SendTree(rawValue: c["tree"] as? String ?? "")),
                holder: try XCTUnwrap(SendHolder(rawValue: c["holder"] as? String ?? "")),
                reader: c["reader"] as? Bool ?? false,
                keyArrives: c["keyArrives"] as? Bool ?? false
            )
            XCTAssertEqual(wire.rawValue, c["wire"] as? String, name)
        }
    }

    func testWriterCasesThroughTheWriter() async throws {
        let cases = try XCTUnwrap(try Self.clearTree()["writerCases"] as? [[String: Any]])
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let tree = try XCTUnwrap(c["tree"] as? String)
            let reader = c["reader"] as? Bool ?? false
            let server = Server(reader: reader, keyFrom: (c["keyArrives"] as? Bool ?? false) ? 2 : nil)
            let root: SealRoot? = switch tree {
            case "clear": SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: nil, clearTree: true)
            case "keyed": SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: TreeSealKey(publicKey: "tree-key", epoch: 2))
            case "keyless": SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: nil)
            default: nil
            }
            let input: SessionsSendInput = tree == "new"
                ? .newChat(teamId: "t1", deviceId: "d", text: "hi")
                : .message(sessionId: "r1", text: "hi")
            let writer = sealing(server, holder: try XCTUnwrap(c["holder"] as? String))
            var prepared: PreparedSend?
            var failure: Error?
            do {
                prepared = try await writer.prepare(accountId: "acc", input, root: root, mentions: nil, deviceIds: { _ in [] })
            } catch {
                failure = error
            }
            let sent = prepared?.input
            switch try XCTUnwrap(c["wire"] as? String) {
            case "plain":
                XCTAssertEqual(sent?.input.text, "hi", name)
                XCTAssertNil(sent?.seal, name)
                XCTAssertEqual(server.asks.value, [], name)
            case "inline":
                XCTAssertEqual(sent?.seal?.epoch, 1, name)
                XCTAssertEqual(sent?.input.text, "exp-sealed:new-tree:1:hi", name)
            case "create":
                XCTAssertEqual(server.asks.value, ["create"], name)
                XCTAssertNil(sent?.seal, name)
                XCTAssertEqual(sent?.input.text, "exp-sealed:new-tree:1:hi", name)
            case "sealed":
                XCTAssertNil(sent?.seal, name)
                XCTAssertEqual(sent?.input.text, tree == "keyed" ? "exp-sealed:tree-key:2:hi" : "exp-sealed:device-tree:1:hi", name)
            default:
                XCTAssertNil(prepared, name)
                XCTAssertEqual(failure as? SealRefusal, SealRefusal(), name)
                XCTAssertEqual(failure?.localizedDescription, noKeyRefusal, name)
            }
        }
    }

    func testAnAttachmentOnlySendIsNeverRefused() async throws {
        let writer = sealing(Server(reader: true, keyFrom: nil), holder: "none")
        let prepared = try await writer.prepare(
            accountId: "acc", .message(sessionId: "r1", text: nil, attachmentIds: ["a1"]),
            root: SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: nil), mentions: nil, deviceIds: { _ in [] }
        )
        XCTAssertNil(prepared.input.input.text)
        XCTAssertEqual(prepared.input.input.attachmentIds, ["a1"])
    }

    func testAClearTreeRowDecodesAndAbsentIsSealed() throws {
        let clear = try JSONDecoder().decode(SessionEntity.self, from: Data(#"""
            {"id":"s-1","team_id":"t","user_id":"u","device_id":"d","clear_tree":"true","created_at":"x","updated_at":"x"}
            """#.utf8))
        XCTAssertTrue(clear.clearTree)
        let legacy = try JSONDecoder().decode(SessionEntity.self, from: Data(#"""
            {"id":"s-2","team_id":"t","user_id":"u","device_id":"d","created_at":"x","updated_at":"x"}
            """#.utf8))
        XCTAssertFalse(legacy.clearTree)
    }
}
