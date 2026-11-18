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

    /// `creates` = each `sessionKeys.create`'s refusal in turn (nil = it is
    /// accepted; the last one repeats); `answer` = `sessionKeys.requestKey`'s
    /// (nil = it throws `requestError`); `keyFrom` = the publicKeys call from
    /// which on the device's key is there (nil = never).
    private final class Server: SessionKeysRegistry, @unchecked Sendable {
        let creates: [TrpcError?]
        let answer: RequestKeyResult?
        let requestError: TrpcError
        let keyFrom: Int?
        let asks = Locked<[String]>([])
        let keyCalls = Locked(0)
        let createdWraps = Locked<[[String]]>([])
        init(creates: [TrpcError?], answer: RequestKeyResult? = RequestKeyResult(state: "requested"), requestError: TrpcError = ClearTreeFixtureTests.notFound, keyFrom: Int?) {
            self.creates = creates
            self.answer = answer
            self.requestError = requestError
            self.keyFrom = keyFrom
        }
        /// `reader` = a create is accepted, else NOT_FOUND (this install may not key it).
        convenience init(reader: Bool, keyFrom: Int?) {
            self.init(creates: [reader ? nil : ClearTreeFixtureTests.notFound], keyFrom: keyFrom)
        }

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            let n = asks.value.filter { $0 == "create" }.count
            asks.mutate { $0.append("create") }
            createdWraps.mutate { $0.append(input.wraps.map(\.installId)) }
            if let refusal = creates[min(n, creates.count - 1)] { throw refusal }
        }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] { [] }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { [] }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {}
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            keyCalls.mutate { $0 += 1 }
            guard let keyFrom, keyCalls.value >= keyFrom else { return [] }
            return rootIds.map { TreePublicKey(rootId: $0, epoch: 1, publicKey: "device-tree") }
        }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult {
            asks.mutate { $0.append("requestKey") }
            guard let answer else { throw requestError }
            return answer
        }
    }

    /// tRPC refusals as the server sends them.
    static func trpc(_ status: Int, _ code: String, _ message: String) -> TrpcError {
        .httpError(status, #"{"error":{"message":"\#(message)","data":{"code":"\#(code)"}}}"#)
    }
    static let notFound = trpc(404, "NOT_FOUND", "Session not found")

    private static func clearTree() throws -> [String: Any] {
        try XCTUnwrap(try SealedContentFixtureTests.fixture()["clearTree"] as? [String: Any])
    }

    /// `holder`: `ready` = registered, `late` = registered at the second ask,
    /// `none` = never.
    private func sealing(_ server: Server, holder: String, keyWait: Duration = .milliseconds(1500)) -> SendSealing {
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
            keyWait: keyWait
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

    // MARK: a keyless tree (#EXP-1960, `keylessCases`)

    func testKeylessConstantsAreTheFixtures() throws {
        let f = try Self.clearTree()
        XCTAssertEqual(f["createAttempts"] as? Int, sealCreateAttempts)
        XCTAssertEqual(sealCreateRetry, .milliseconds(try XCTUnwrap(f["createRetryMs"] as? Int)))
        XCTAssertEqual(f["wrapRefusal"] as? String, wrapRefusal)
    }

    func testKeylessCasesPure() throws {
        let cases = try XCTUnwrap(try Self.clearTree()["keylessCases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 15)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let step: KeylessStep = if let answer = c["answer"] as? String {
                keylessStep(answer: try XCTUnwrap(RequestKeyAnswer(rawValue: answer), name))
            } else {
                keylessStep(
                    failure: try XCTUnwrap(CreateFailure(rawValue: c["failure"] as? String ?? ""), name),
                    attempt: c["attempt"] as? Int ?? 1,
                    ownOnly: c["ownOnly"] as? Bool ?? false
                )
            }
            XCTAssertEqual(step.rawValue, c["step"] as? String, name)
        }
    }

    func testCreateFailureReadsTheTrpcError() {
        XCTAssertEqual(createFailure(code: "BAD_REQUEST", message: clearTreeRefusal), .clear)
        XCTAssertEqual(createFailure(code: "CONFLICT", message: staleTreeRefusal), .stale)
        XCTAssertEqual(createFailure(code: "CONFLICT", message: "The tree is at epoch 2"), .stale)
        XCTAssertEqual(createFailure(code: "BAD_REQUEST", message: wrapRefusal), .wrapRefused)
        XCTAssertEqual(createFailure(code: "NOT_FOUND", message: "Session not found"), .notReader)
        XCTAssertEqual(createFailure(code: "FORBIDDEN", message: "no"), .refused)
        XCTAssertEqual(createFailure(code: "INTERNAL_SERVER_ERROR", message: "boom"), .transient)
        XCTAssertEqual(createFailure(code: "TOO_MANY_REQUESTS", message: nil), .transient)
        XCTAssertEqual(createFailure(code: nil, message: nil), .transient)
    }

    /// A pre-#EDE-97 chat as the store has it: no key, not clear, real ids.
    private static let legacyRoot = SealRoot(
        id: "0b6f3c2e-8d1a-4f5b-9c7e-2a4d6e8f1b3c", teamId: "5e2a9c41-7b3d-4e6f-8a1c-9d2b4f6e8a0c",
        deviceId: "f3a1c5e7-9b2d-4f6a-8c0e-1d3b5f7a9c2e", shared: true, key: nil
    )

    private func send(_ writer: SendSealing) async -> (PreparedSend?, Error?) {
        do {
            return (try await writer.prepare(
                accountId: "acc", .message(sessionId: Self.legacyRoot.id, text: "hi"), root: Self.legacyRoot, mentions: nil, deviceIds: { _ in [] }
            ), nil)
        } catch {
            return (nil, error)
        }
    }

    func testAnIdleDevicesKeylessChatIsKeyedOnRequest() async throws {
        // Not a reader (NOT_FOUND), its device idle: asked, it keys the tree
        // and the key is there on a later publicKeys poll.
        let server = Server(creates: [Self.notFound], answer: RequestKeyResult(state: "requested"), keyFrom: 3)
        let (prepared, failure) = await send(sealing(server, holder: "ready", keyWait: .seconds(5)))
        XCTAssertNil(failure)
        XCTAssertEqual(server.asks.value, ["create", "requestKey"])
        XCTAssertEqual(server.keyCalls.value, 3)
        XCTAssertNil(prepared?.input.seal)
        XCTAssertEqual(prepared?.input.input.text, "exp-sealed:device-tree:1:hi")
    }

    func testNoHostFailsAtOnce() async throws {
        let server = Server(creates: [Self.notFound], answer: RequestKeyResult(state: "noHost"), keyFrom: nil)
        let started = ContinuousClock.now
        let (prepared, failure) = await send(sealing(server, holder: "ready", keyWait: sealKeyWait))
        XCTAssertLessThan(ContinuousClock.now - started, .seconds(2))
        XCTAssertNil(prepared)
        XCTAssertEqual(failure as? SealRefusal, SealRefusal())
        XCTAssertEqual(server.asks.value, ["create", "requestKey"])
        XCTAssertEqual(server.keyCalls.value, 0)
    }

    func testRequestKeyAnswersKeyedOrClear() async throws {
        let keyed = Server(creates: [Self.notFound], answer: RequestKeyResult(state: "keyed", epoch: 4, publicKey: "host-tree"), keyFrom: nil)
        let (sealed, _) = await send(sealing(keyed, holder: "ready", keyWait: sealKeyWait))
        XCTAssertEqual(sealed?.input.input.text, "exp-sealed:host-tree:4:hi")
        let clear = Server(creates: [Self.notFound], answer: RequestKeyResult(state: "clear"), keyFrom: nil)
        let (plain, _) = await send(sealing(clear, holder: "ready", keyWait: sealKeyWait))
        XCTAssertEqual(plain?.input.input.text, "hi")
    }

    func testAnOlderServerWithoutRequestKeyWaitsBare() async throws {
        let server = Server(creates: [Self.notFound], answer: nil, requestError: Self.trpc(404, "NOT_FOUND", #"No procedure found on path \"sessionKeys.requestKey\""#), keyFrom: 2)
        let (prepared, failure) = await send(sealing(server, holder: "ready"))
        XCTAssertNil(failure)
        XCTAssertEqual(prepared?.input.input.text, "exp-sealed:device-tree:1:hi")
    }

    func testABlipIsRetriedThenSealed() async throws {
        let server = Server(creates: [Self.trpc(500, "INTERNAL_SERVER_ERROR", "boom"), nil], keyFrom: nil)
        let started = ContinuousClock.now
        let (prepared, failure) = await send(sealing(server, holder: "ready", keyWait: sealKeyWait))
        XCTAssertNil(failure)
        XCTAssertGreaterThanOrEqual(ContinuousClock.now - started, sealCreateRetry)
        XCTAssertEqual(server.asks.value, ["create", "create"])
        XCTAssertEqual(prepared?.input.input.text, "exp-sealed:new-tree:1:hi")
    }

    func testBlipsUpToTheLastAttemptFail() async throws {
        let server = Server(creates: [.httpError(502, "<html>Bad Gateway</html>")], keyFrom: nil)
        let (prepared, failure) = await send(sealing(server, holder: "ready", keyWait: sealKeyWait))
        XCTAssertNil(prepared)
        XCTAssertEqual(failure as? SealRefusal, SealRefusal())
        XCTAssertEqual(server.asks.value, Array(repeating: "create", count: sealCreateAttempts))
    }

    func testAWrapRefusalRetriesWithOwnInstallsThenAsksTheDevice() async throws {
        let refusal = Self.trpc(400, "BAD_REQUEST", wrapRefusal)
        let server = Server(creates: [refusal], answer: RequestKeyResult(state: "noHost"), keyFrom: nil)
        let (_, failure) = await send(sealing(server, holder: "ready", keyWait: sealKeyWait))
        XCTAssertEqual(failure as? SealRefusal, SealRefusal())
        XCTAssertEqual(server.asks.value, ["create", "create", "requestKey"])
        let accepted = Server(creates: [refusal, nil], keyFrom: nil)
        let (prepared, _) = await send(sealing(accepted, holder: "ready", keyWait: sealKeyWait))
        XCTAssertEqual(accepted.asks.value, ["create", "create"])
        XCTAssertEqual(accepted.createdWraps.value.last, ["install-me"])
        XCTAssertEqual(prepared?.input.input.text, "exp-sealed:new-tree:1:hi")
    }

    func testAnyOtherRefusalFailsWithoutAWait() async throws {
        let server = Server(creates: [Self.trpc(403, "FORBIDDEN", "no")], keyFrom: nil)
        let started = ContinuousClock.now
        let (_, failure) = await send(sealing(server, holder: "ready", keyWait: sealKeyWait))
        XCTAssertLessThan(ContinuousClock.now - started, .seconds(2))
        XCTAssertEqual(failure as? SealRefusal, SealRefusal())
        XCTAssertEqual(server.asks.value, ["create"])
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
