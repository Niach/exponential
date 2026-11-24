@testable import ExpCore
import Foundation
import XCTest

// #EXP-1951: packages/domain-contract/fixtures/sealed-content.json `rotation`
// replayed: the pure `rotationEpoch` / `rotationAfter`, and `holderCases` =
// one `TreeKeyHolder` pass against a SERVER DOUBLE that answers what each
// case says. The double may keep serving a row wrapped for another install
// (`serves: foreign`); the holder wraps a new epoch for itself and for the
// pairs `pending` lists, nobody else.
final class RotationFixtureTests: XCTestCase {
    private static func rotation() throws -> [String: Any] {
        try XCTUnwrap(try SealedContentFixtureTests.fixture()["rotation"] as? [String: Any])
    }

    func testStepCases() throws {
        let cases = try XCTUnwrap(try Self.rotation()["stepCases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 7)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let epoch = rotationEpoch(
                flagged: (c["flagged"] as? Int).map(UInt32.init), current: (c["current"] as? Int).map(UInt32.init),
                held: try XCTUnwrap(c["held"] as? Bool), left: try XCTUnwrap(c["left"] as? Bool)
            )
            XCTAssertEqual(epoch, (c["epoch"] as? Int).map(UInt32.init), name)
        }
    }

    func testAnswerCases() throws {
        let cases = try XCTUnwrap(try Self.rotation()["answerCases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 7)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let failure = try (c["failure"] as? String).map { try XCTUnwrap(CreateFailure(rawValue: $0), name) }
            XCTAssertEqual(rotationAfter(failure).rawValue, c["then"] as? String, name)
        }
    }

    /// The install of the fixture over a transparent wrap (`w|<public>|<hex>`),
    /// so the replay runs without the FFI; the fixture's two real wraps open
    /// as the fixture says (the other holder's key; a foreign row never).
    private final class Crypto: SealedCrypto, @unchecked Sendable {
        let install: (secret: Data, publicKey: String)
        let rotatedWrap: String
        let rotatedSecret: Data
        let made = Locked(0)
        init(install: (secret: Data, publicKey: String), rotatedWrap: String, rotatedSecret: Data) {
            self.install = install
            self.rotatedWrap = rotatedWrap
            self.rotatedSecret = rotatedSecret
        }
        var isAvailable: Bool { true }
        func installKey(identity: Data) throws -> (secret: Data, publicKey: String) { install }
        func generateTreeKey() -> GeneratedTreeKey {
            made.mutate { $0 += 1 }
            return GeneratedTreeKey(secret: Data(repeating: UInt8(0xE0 + made.value), count: 32), publicKey: "fresh-\(made.value)")
        }
        func wrap(installPublic: String, treeSecret: Data) throws -> String {
            "w|\(installPublic)|\(treeSecret.map { String(format: "%02x", $0) }.joined())"
        }
        func unwrap(installSecret: Data, wrapped: String) throws -> Data {
            if wrapped == rotatedWrap { return rotatedSecret }
            let prefix = "w|\(install.publicKey)|"
            guard wrapped.hasPrefix(prefix) else { throw TreeKeyError.unavailable }
            return try SealedContentFixtureTests.hex(String(wrapped.dropFirst(prefix.count)))
        }
        func sealBody(treePublicKey: String, epoch: UInt32, text: String) throws -> String { text }
        func openBody(_ body: String, keys: [UInt32: Data]) -> OpenedText { .plain(body) }
        func sealCard(treePublicKey: String, epoch: UInt32, cardJson: String) throws -> String { cardJson }
        func openCard(_ cardJson: String, keys: [UInt32: Data]) -> OpenedCardJson { OpenedCardJson(status: .plain, epoch: nil, json: cardJson) }
    }

    private final class ServerDouble: SessionKeysRegistry, @unchecked Sendable {
        struct Create: Equatable { let rootId: String; let epoch: Int; let wraps: [String] }
        struct Wrap: Equatable { let rootId: String; let epoch: Int; let to: String }
        let due: [DueTreeKey]
        let readers: [String]
        let heldRoots: [String]
        let foreign: String?
        let rotatedWrap: String
        let answers: Locked<[String]>
        let done = Locked<Set<String>>([])
        let conflicted = Locked<[String: UInt32]>([:])
        let created = Locked<[(rootId: String, epoch: UInt32, publicKey: String)]>([])
        let creates = Locked<[Create]>([])
        let wraps = Locked<[Wrap]>([])

        init(case c: [String: Any], rotation: [String: Any]) {
            due = (c["due"] as? [[String: Any]] ?? []).map { DueTreeKey(rootId: $0["rootId"] as! String, epoch: UInt32($0["epoch"] as! Int)) }
            readers = c["readers"] as? [String] ?? []
            heldRoots = Array((c["held"] as? [String: Any] ?? [:]).keys)
            foreign = (c["serves"] as? String) == "foreign" ? rotation["foreignWrap"] as? String : nil
            rotatedWrap = rotation["rotatedWrap"] as! String
            answers = Locked(c["create"] as? [String] ?? [])
        }

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }
        func due(accountId: String, installId: String) async throws -> [DueTreeKey] {
            due.filter { !done.value.contains($0.rootId) }
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            creates.mutate { $0.append(Create(rootId: input.rootId, epoch: Int(input.epoch), wraps: input.wraps.map { $0.installId == installId ? "self" : $0.installId })) }
            var answer = "ok"
            answers.mutate { if !$0.isEmpty { answer = $0.removeFirst() } }
            switch answer {
            case "ok":
                done.mutate { $0.insert(input.rootId) }
                created.mutate { $0.append((input.rootId, input.epoch, input.publicKey)) }
            case "blip":
                throw URLError(.notConnectedToInternet)
            case "CONFLICT":
                done.mutate { $0.insert(input.rootId) }
                conflicted.mutate { $0[input.rootId] = input.epoch }
                throw ClearTreeFixtureTests.trpc(409, "CONFLICT", "The tree is at epoch \(input.epoch)")
            case "FORBIDDEN":
                throw ClearTreeFixtureTests.trpc(403, "FORBIDDEN", "Only the session's starter or its device rotates the key")
            default:
                throw ClearTreeFixtureTests.trpc(404, answer, "Session not found")
            }
        }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
            var rows = foreign.map { wrap in heldRoots.map { HeldTreeKey(rootId: $0, epoch: 9, wrapped: wrap) } } ?? []
            rows += conflicted.value.map { HeldTreeKey(rootId: $0.key, epoch: $0.value, wrapped: rotatedWrap) }
            return rows.filter { rootIds?.contains($0.rootId) ?? true }
        }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] {
            created.value.flatMap { key in
                readers.filter { to in !wraps.value.contains { $0.rootId == key.rootId && $0.to == to } }
                    .map { PendingTreeKey(rootId: key.rootId, epoch: key.epoch, publicKey: key.publicKey, targetInstallId: $0, targetX25519Public: "public-of-\($0)") }
            }
        }
        func wrap(accountId: String, installId: String, wraps sent: [PendingTreeKeyWrap]) async throws {
            wraps.mutate { $0 += sent.map { Wrap(rootId: $0.rootId, epoch: Int($0.epoch), to: $0.targetInstallId) } }
        }
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] { [] }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult { RequestKeyResult(state: "noHost") }

        let installId = "install-me"
    }

    func testHolderCases() async throws {
        let f = try SealedContentFixtureTests.fixture()
        let rotation = try Self.rotation()
        let install = try XCTUnwrap(f["install"] as? [String: Any])
        let secretAt: [UInt32: Data] = [
            1: try SealedContentFixtureTests.hex(try XCTUnwrap(f["tree"] as? [String: Any])["secret"]),
            2: try SealedContentFixtureTests.hex(try XCTUnwrap(f["rotatedTree"] as? [String: Any])["secret"]),
        ]
        let cases = try XCTUnwrap(rotation["holderCases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 10)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let crypto = Crypto(
                install: (try SealedContentFixtureTests.hex(install["secret"]), try XCTUnwrap(install["x25519Public"] as? String)),
                rotatedWrap: try XCTUnwrap(rotation["rotatedWrap"] as? String), rotatedSecret: try XCTUnwrap(secretAt[2])
            )
            let server = ServerDouble(case: c, rotation: rotation)
            let ring = SealedKeyRing(persistence: .memory)
            let held = try XCTUnwrap(c["held"] as? [String: [Int]])
            ring.add(held.flatMap { root, epochs in epochs.map { (rootId: root, epoch: UInt32($0), secret: secretAt[UInt32($0)]!) } })
            let holder = TreeKeyHolder(
                keys: server, installs: nil, crypto: crypto, ring: ring,
                identity: { Data(repeating: 1, count: 32) }, installId: { _ in "install-me" }
            )
            for _ in 0..<(c["passes"] as? Int ?? 1) { await holder.sync(accountId: "a") }

            let creates = try XCTUnwrap(c["creates"] as? [[String: Any]]).map { ServerDouble.Create(rootId: $0["rootId"] as! String, epoch: $0["epoch"] as! Int, wraps: $0["wraps"] as! [String]) }
            XCTAssertEqual(server.creates.value, creates, name)
            let wraps = try XCTUnwrap(c["wraps"] as? [[String: Any]]).map { ServerDouble.Wrap(rootId: $0["rootId"] as! String, epoch: $0["epoch"] as! Int, to: $0["to"] as! String) }
            XCTAssertEqual(server.wraps.value, wraps, name)
            let holds = try XCTUnwrap(c["holds"] as? [String: [Int]])
            for root in Set(held.keys).union(holds.keys) {
                XCTAssertEqual(ring.keys(rootId: root).keys.map(Int.init).sorted(), holds[root] ?? [], "\(name): \(root)")
            }
        }
    }
}
