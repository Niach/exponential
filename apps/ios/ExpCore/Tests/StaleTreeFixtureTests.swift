@testable import ExpCore
import Foundation
import XCTest

// #FEED-250: packages/domain-contract/fixtures/sealed-content.json `staleTree`
// replayed through `SendSealing` + `StaleTreeResend` over a fake server: the
// synced store is a cache of the tree, never its authority.
final class StaleTreeFixtureTests: XCTestCase {
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

    /// The tree as the server has it (`none` | `keyless` | `keyed`), and every
    /// call the writer makes of it.
    private final class Server: SessionKeysRegistry, @unchecked Sendable {
        let tree: String
        let asks = Locked<[String]>([])
        init(_ tree: String) { self.tree = tree }

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            asks.mutate { $0.append("sendTarget") }
            switch tree {
            case "none": return ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
            case "keyless": return ServerSendTarget(rootId: "r1", epoch: nil, publicKey: nil)
            default: return ServerSendTarget(rootId: "r1", epoch: 1, publicKey: "server-tree")
            }
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            asks.mutate { $0.append("create") }
            if tree == "keyed" { throw StaleTreeFixtureTests.refused }
        }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] { [] }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { [] }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {}
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            asks.mutate { $0.append("publicKeys") }
            return tree == "keyed" ? rootIds.map { TreePublicKey(rootId: $0, epoch: 1, publicKey: "server-tree") } : []
        }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult {
            asks.mutate { $0.append("requestKey") }
            return RequestKeyResult(state: "noHost")
        }
    }

    private static func stale() throws -> [String: Any] {
        try XCTUnwrap(try SealedContentFixtureTests.fixture()["staleTree"] as? [String: Any])
    }

    /// The server's CONFLICT as tRPC sends it.
    static let refused = refusal(staleTreeRefusal)
    static func refusal(_ message: String) -> TrpcError {
        .httpError(409, #"{"error":{"message":"\#(message)","data":{"code":"CONFLICT"}}}"#)
    }

    private func sealing(_ server: Server) -> SendSealing {
        let crypto = StubCrypto()
        return SendSealing(
            holder: TreeKeyHolder(
                keys: server, installs: nil, crypto: crypto, ring: SealedKeyRing(persistence: .memory),
                identity: { Data(repeating: 3, count: 32) }, installId: { _ in "install-me" }
            ),
            crypto: crypto
        )
    }

    func testTheRefusalIsTheFixtures() throws {
        XCTAssertEqual(try Self.stale()["refusal"] as? String, staleTreeRefusal)
        XCTAssertTrue(isStaleTreeRefusal(staleTreeRefusal))
        XCTAssertTrue(isStaleTreeRefusal("The tree is at epoch 3"))
        XCTAssertFalse(isStaleTreeRefusal("The tree is at epoch 3, really"))
        XCTAssertFalse(isStaleTreeRefusal("Session not found"))
        // An older server's raw epoch never reaches a surface.
        XCTAssertEqual(Self.refusal("The tree is at epoch 1").errorDescription, staleTreeRefusal)
    }

    func testCasesAskTheServerWhereTheStoreCannotKnow() async throws {
        let cases = try XCTUnwrap(try Self.stale()["cases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 5)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let server = Server(try XCTUnwrap(c["server"] as? String))
            let root: SealRoot? = switch c["local"] as? String {
            case "keyed": SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: TreeSealKey(publicKey: "server-tree", epoch: 1))
            case "keyless": SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: nil)
            default: nil
            }
            let input: SessionsSendInput = c["send"] as? String == "agent"
                ? .agent(agentId: "a1", text: "hi")
                : .message(sessionId: "r1", text: "hi")
            let prepared = try await sealing(server).prepare(accountId: "acc", input, root: root, mentions: nil, deviceIds: { _ in [] })
            XCTAssertEqual(server.asks.value, c["asks"] as? [String], name)
            switch c["wire"] as? String {
            case "inline":
                XCTAssertEqual(prepared.input.seal?.epoch, 1, name)
                XCTAssertEqual(prepared.input.input.text, "exp-sealed:new-tree:1:hi", name)
            case "create":
                XCTAssertNil(prepared.input.seal, name)
                XCTAssertEqual(prepared.input.input.text, "exp-sealed:new-tree:1:hi", name)
            default:
                XCTAssertNil(prepared.input.seal, name)
                XCTAssertEqual(prepared.input.input.text, "exp-sealed:server-tree:1:hi", name)
            }
        }
    }

    func testRetryCasesResealOnceByThemselves() async throws {
        let cases = try XCTUnwrap(try Self.stale()["retryCases"] as? [[String: Any]])
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            // The store missed the keyed conversation; the server knows it.
            let sealing = sealing(Server("keyed"))
            var refusals = try XCTUnwrap(c["refusals"] as? Int)
            var sent: [SealedSendInput] = []
            var outcome = "sent"
            do {
                _ = try await StaleTreeResend.send { fresh in
                    let prepared = try await sealing.prepare(
                        accountId: "acc", .agent(agentId: "a1", text: "hi"), root: nil, mentions: nil, deviceIds: { _ in [] }, fresh: fresh
                    )
                    sent.append(prepared.input)
                    if refusals > 0 {
                        refusals -= 1
                        throw Self.refused
                    }
                    return SessionsSendResult(sessionId: "r1")
                }
            } catch {
                outcome = "failed"
                XCTAssertEqual(error.localizedDescription, staleTreeRefusal, name)
            }
            XCTAssertEqual(outcome, c["outcome"] as? String, name)
            XCTAssertEqual(sent.count, c["sends"] as? Int, name)
            // The re-send is sealed to the server's key, never a new inline one.
            XCTAssertNil(sent.last?.seal, name)
            XCTAssertEqual(sent.last?.input.text, "exp-sealed:server-tree:1:hi", name)
        }
    }

    func testAFreshResolveAsksTheServerAndOtherFailuresAreNotRetried() async throws {
        let server = Server("keyed")
        let local = SealRoot(id: "r1", teamId: "t1", deviceId: "d", shared: false, key: TreeSealKey(publicKey: "stale", epoch: 1))
        let prepared = try await sealing(server).prepare(
            accountId: "acc", .agent(agentId: "a1", text: "hi"), root: local, mentions: nil, deviceIds: { _ in [] }, fresh: true
        )
        XCTAssertEqual(server.asks.value, ["sendTarget"])
        XCTAssertEqual(prepared.input.input.text, "exp-sealed:server-tree:1:hi")

        var calls = 0
        do {
            _ = try await StaleTreeResend.send { _ -> SessionsSendResult in
                calls += 1
                throw Self.refusal("Session not found")
            }
            XCTFail("expected the failure")
        } catch {
            XCTAssertEqual(error.localizedDescription, "Session not found")
        }
        XCTAssertEqual(calls, 1)
    }
}
