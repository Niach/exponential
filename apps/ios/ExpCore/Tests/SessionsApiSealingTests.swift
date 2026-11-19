@testable import ExpCore
import Foundation
import GRDB
import XCTest

// #EXP-1956: a chat's own composer sends through the install's
// `SessionsApi` (the one whose slot holds the sealer): a follow-up into a
// keyed tree leaves sealed, a keyless root is keyed first, never plaintext.
// Through `SessionsApi.send` over a stand-in server.
final class SessionsApiSealingTests: XCTestCase {
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

    private final class Keys: SessionKeysRegistry, @unchecked Sendable {
        let creates = Locked<[String]>([])
        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            creates.mutate { $0.append(input.rootId) }
        }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] { [] }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { [] }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {}
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] { [] }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult { RequestKeyResult(state: "noHost") }
    }

    /// Records each `sessions.send` body and answers it.
    private final class Server: URLProtocol {
        static let bodies = Locked<[[String: Any]]>([])

        override class func canInit(with request: URLRequest) -> Bool { true }
        override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

        override func startLoading() {
            let data = request.httpBody ?? request.httpBodyStream.map(Self.read) ?? Data()
            if let body = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] {
                Self.bodies.mutate { $0.append(body) }
            }
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Data(#"{"result":{"data":{"sessionId":"s","messageId":"m"}}}"#.utf8))
            client?.urlProtocolDidFinishLoading(self)
        }

        override func stopLoading() {}

        private static func read(_ stream: InputStream) -> Data {
            stream.open()
            defer { stream.close() }
            var data = Data()
            var buffer = [UInt8](repeating: 0, count: 4096)
            while stream.hasBytesAvailable {
                let n = stream.read(&buffer, maxLength: buffer.count)
                if n <= 0 { break }
                data.append(buffer, count: n)
            }
            return data
        }
    }

    private final class FakeKeychain: KeychainStoring, @unchecked Sendable {
        private let lock = NSLock()
        private var storage: [String: String] = [:]
        func get(_ key: String) throws -> String? { lock.withLock { storage[key] } }
        @discardableResult
        func set(_ key: String, value: String?) -> Bool {
            lock.withLock { storage[key] = value }
            return true
        }
        func delete(_ key: String) { lock.withLock { storage[key] = nil } }
    }

    func testAChatFollowUpLeavesSealedAndAKeylessRootIsKeyedFirst() async throws {
        let auth = AuthRepository(accountStore: AccountStore(keychain: FakeKeychain()))
        auth.setInstanceUrl("https://exp.example.com")
        auth.setToken("t", email: "u@x.com", userId: "u", onboardingKnown: true)
        let accountId = ServerAccount.makeId(instanceUrl: "https://exp.example.com", userId: "u")
        let db = DatabaseManager()
        defer {
            db.closePool(forAccountId: accountId)
            DatabaseManager.deleteFiles(forAccountId: accountId)
        }
        let pool = try db.pool(forAccountId: accountId)
        try DatabaseManager.runMigrations(on: pool)
        let rows = [
            #"{"id":"keyed","team_id":"t","user_id":"u","device_id":"d","seal_public_key":"tree-key","seal_epoch":2,"created_at":"x","updated_at":"x"}"#,
            #"{"id":"keyless","team_id":"t","user_id":"u","device_id":"d","created_at":"x","updated_at":"x"}"#,
        ]
        try await pool.write { conn in
            for row in rows { try JSONDecoder().decode(SessionEntity.self, from: Data(row.utf8)).save(conn) }
        }

        let crypto = StubCrypto()
        let keys = Keys()
        let api = SessionsApi(trpc: TrpcClient(httpClient: HTTPClient(auth: auth, protocolClasses: [Server.self]), auth: auth))
        api.sealing.sealer = SessionSealer(
            holder: TreeKeyHolder(
                keys: keys, installs: nil, crypto: crypto, ring: SealedKeyRing(persistence: .memory),
                identity: { Data(repeating: 3, count: 32) }, installId: { _ in "install-me" }
            ),
            opener: SealedOpener(crypto: crypto, ring: SealedKeyRing(persistence: .memory)),
            db: db,
            userId: { _ in "u" }
        )
        Server.bodies.mutate { $0 = [] }

        try await api.send(accountId: accountId, .message(sessionId: "keyed", text: "follow-up"), clientMessageId: "c1")
        try await api.send(accountId: accountId, .message(sessionId: "keyless", text: "keyless test"), clientMessageId: "c2")

        let sent = Server.bodies.value
        XCTAssertEqual(sent.map { $0["text"] as? String }, ["exp-sealed:tree-key:2:follow-up", "exp-sealed:new-tree:1:keyless test"])
        XCTAssertEqual(keys.creates.value, ["keyless"], "the keyless root is keyed before the send")
        XCTAssertEqual(sent.map { $0["clientMessageId"] as? String }, ["c1", "c2"])
    }

    /// #EXP-1958 (sealed-content.json `cardStamps`): a tap on a sealed card
    /// goes through the same writer: the text sealed, a Start's `name` as
    /// `sealedName` (no clear `name`), `cardItem` passed through untouched.
    func testASealedCardTapLeavesSealedWithItsCardItem() async throws {
        let auth = AuthRepository(accountStore: AccountStore(keychain: FakeKeychain()))
        auth.setInstanceUrl("https://exp.example.com")
        auth.setToken("t", email: "u@x.com", userId: "u", onboardingKnown: true)
        let accountId = ServerAccount.makeId(instanceUrl: "https://exp.example.com", userId: "u")
        let db = DatabaseManager()
        defer {
            db.closePool(forAccountId: accountId)
            DatabaseManager.deleteFiles(forAccountId: accountId)
        }
        let pool = try db.pool(forAccountId: accountId)
        try DatabaseManager.runMigrations(on: pool)
        let row = #"{"id":"keyed","team_id":"t","user_id":"u","device_id":"d","seal_public_key":"tree-key","seal_epoch":2,"created_at":"x","updated_at":"x"}"#
        try await pool.write { conn in
            try JSONDecoder().decode(SessionEntity.self, from: Data(row.utf8)).save(conn)
        }
        let crypto = StubCrypto()
        let api = SessionsApi(trpc: TrpcClient(httpClient: HTTPClient(auth: auth, protocolClasses: [Server.self]), auth: auth))
        api.sealing.sealer = SessionSealer(
            holder: TreeKeyHolder(
                keys: Keys(), installs: nil, crypto: crypto, ring: SealedKeyRing(persistence: .memory),
                identity: { Data(repeating: 3, count: 32) }, installId: { _ in "install-me" }
            ),
            opener: SealedOpener(crypto: crypto, ring: SealedKeyRing(persistence: .memory)),
            db: db,
            userId: { _ in "u" }
        )
        Server.bodies.mutate { $0 = [] }
        let issue = "00000000-0000-4000-8000-000000000012"

        try await api.send(
            accountId: accountId, CardItemWire.message(sessionId: "keyed", text: "Yes, ship it"),
            cardItem: CardItemRef(messageId: "m-1", itemId: "r1")
        )
        try await api.send(
            accountId: accountId,
            CardItemWire.start(parentId: "keyed", title: "Add tests", text: "Cover it", issueIds: [issue, "EXP-9"], model: "opus"),
            cardItem: CardItemRef(messageId: "m-2", itemId: "s1")
        )

        let sent = Server.bodies.value
        XCTAssertEqual(sent.count, 2)
        XCTAssertEqual(sent[0]["text"] as? String, "exp-sealed:tree-key:2:Yes, ship it")
        XCTAssertEqual(sent[0]["cardItem"] as? [String: String], ["messageId": "m-1", "itemId": "r1"])
        XCTAssertEqual(sent[1].keys.sorted(), ["cardItem", "issueIds", "model", "parentId", "sealedName", "text"])
        XCTAssertEqual(sent[1]["text"] as? String, "exp-sealed:tree-key:2:Add tests\n\nCover it")
        XCTAssertEqual(sent[1]["sealedName"] as? String, "exp-sealed:tree-key:2:Add tests")
        XCTAssertEqual(sent[1]["issueIds"] as? [String], [issue])
        XCTAssertEqual(sent[1]["cardItem"] as? [String: String], ["messageId": "m-2", "itemId": "s1"])
    }
}
