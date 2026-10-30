@testable import ExpCore
import Foundation
import XCTest

// EXP-1457 step 1: the install registration wire (VAPP-37 contract) and the
// node service's rules, over a fake registry and a fake node.
final class InstallNodeServiceTests: XCTestCase {
    func testMessageIsTheContractString() {
        XCTAssertEqual(
            InstallRegistration.message(userId: "u1", endpointId: "ab", x25519Public: "cd", signedAt: "2026-10-30T12:00:00.000Z"),
            "exp-install-v1\nu1\nab\ncd\n2026-10-30T12:00:00.000Z"
        )
    }

    func testSignedAtIsIsoUtcWithMilliseconds() {
        XCTAssertEqual(InstallRegistration.signedAt(Date(timeIntervalSince1970: 1_793_361_600.25)), "2026-10-30T12:00:00.250Z")
    }

    func testInputSignsTheMessageWithTheIdentity() throws {
        let crypto = FakeCrypto()
        let input = try InstallRegistration.input(
            identity: Data(repeating: 7, count: 32), userId: "u1", crypto: crypto,
            label: String(repeating: "x", count: 80), platform: "iOS 27.1",
            now: Date(timeIntervalSince1970: 1_793_361_600)
        )
        XCTAssertEqual(input.kind, "ios")
        XCTAssertEqual(input.endpointId, crypto.endpointId)
        XCTAssertEqual(input.x25519Public, "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE")
        XCTAssertEqual(input.signedAt, "2026-10-30T12:00:00.000Z")
        XCTAssertEqual(input.label?.count, 64)
        let signed = try XCTUnwrap(crypto.signed.last)
        XCTAssertEqual(String(data: signed, encoding: .utf8), InstallRegistration.message(
            userId: "u1", endpointId: crypto.endpointId, x25519Public: input.x25519Public, signedAt: input.signedAt
        ))
        XCTAssertEqual(input.signature, InstallRegistration.base64Url(FakeCrypto.signature))
    }

    func testRegisterInputOmitsAbsentOptionals() throws {
        let input = RegisterInstallInput(kind: "ios", label: nil, platform: nil, endpointId: "e", x25519Public: "x", signedAt: "s", signature: "g")
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(input)) as? [String: Any])
        XCTAssertEqual(Set(json.keys), ["kind", "endpointId", "x25519Public", "signedAt", "signature"])
    }

    func testAllowlistDiff() {
        let lists = [[entry("a"), entry("me")], [entry("b"), entry("a")]]
        let desired = InstallAllowlist.desired(lists, own: "me")
        XCTAssertEqual(desired, ["a", "b"])
        let (allow, revoke) = InstallAllowlist.diff(current: ["a", "gone"], desired: desired)
        XCTAssertEqual(allow, ["b"])
        XCTAssertEqual(revoke, ["gone"])
    }

    func testSyncStartsRegistersOnceAndLoadsEveryTeam() async throws {
        let registry = FakeRegistry(teams: ["t1": [entry("peer1"), entry("self")], "t2": [entry("peer2")]])
        let harness = Harness(registry: registry)
        let account = InstallAccount(accountId: "acc", userId: "u1", teamIds: ["t1", "t2"])
        await harness.service.sync([account])
        XCTAssertEqual(registry.registered.count, 1)
        XCTAssertEqual(harness.started.count, 1)
        XCTAssertEqual(Set(harness.node.allowedPeers()), ["peer1", "peer2"])
        let installId = await harness.service.installId(accountId: "acc")
        XCTAssertEqual(installId, "install-1")

        // The 10-minute refresh: no second register, a removed peer is revoked.
        registry.teams["t2"] = []
        await harness.service.sync([account])
        XCTAssertEqual(registry.registered.count, 1)
        XCTAssertEqual(harness.started.count, 1)
        XCTAssertEqual(harness.node.allowedPeers(), ["peer1"])
    }

    func testAFailedListNeverRevokes() async {
        let registry = FakeRegistry(teams: ["t1": [entry("peer1")]])
        let harness = Harness(registry: registry)
        let account = InstallAccount(accountId: "acc", userId: "u1", teamIds: ["t1"])
        await harness.service.sync([account])
        registry.failList = true
        await harness.service.refreshAllowlist()
        XCTAssertEqual(harness.node.allowedPeers(), ["peer1"])
    }

    func testARevokedEndpointRotatesTheIdentityAndRegistersAgain() async {
        let registry = FakeRegistry(teams: [:])
        registry.forbidOnce = true
        let harness = Harness(registry: registry)
        await harness.service.sync([InstallAccount(accountId: "acc", userId: "u1", teamIds: [])])
        XCTAssertEqual(harness.rotations.count, 1)
        XCTAssertEqual(harness.started.count, 2)
        XCTAssertEqual(registry.registered.count, 1)
        XCTAssertNotEqual(harness.started[0], harness.started[1])
    }

    func testSigningOutTheLastAccountRevokesStopsAndForgets() async {
        let registry = FakeRegistry(teams: ["t1": [entry("peer1")]])
        let harness = Harness(registry: registry)
        await harness.service.sync([InstallAccount(accountId: "acc", userId: "u1", teamIds: ["t1"])])
        await harness.service.signOut(accountId: "acc", revoke: true)
        XCTAssertEqual(registry.revoked, ["install-1"])
        XCTAssertTrue(harness.node.isShutDown)
        XCTAssertEqual(harness.rotations.count, 1)
        let endpoint = await harness.service.endpointId
        XCTAssertNil(endpoint)
    }

    func testNoAccountsStopsTheNodeAndKeepsTheIdentity() async {
        let harness = Harness(registry: FakeRegistry(teams: [:]))
        await harness.service.sync([InstallAccount(accountId: "acc", userId: "u1", teamIds: [])])
        await harness.service.sync([])
        XCTAssertTrue(harness.node.isShutDown)
        XCTAssertEqual(harness.rotations.count, 0)
    }
}

private func entry(_ endpoint: String) -> InstallEntry {
    InstallEntry(installId: "i-\(endpoint)", userId: "u", kind: "ios", endpointId: endpoint, x25519Public: "x")
}

private final class FakeCrypto: InstallCrypto, @unchecked Sendable {
    static let signature = Data(repeating: 9, count: 64)
    let endpointId = String(repeating: "ab", count: 32)
    private let lock = NSLock()
    private var _signed: [Data] = []
    var signed: [Data] { lock.withLock { _signed } }

    func endpointId(identity: Data) throws -> String { identity == Data(repeating: 7, count: 32) ? endpointId : "self" }
    func x25519Public(identity: Data) throws -> Data { Data(repeating: 1, count: 32) }
    func sign(identity: Data, message: Data) throws -> Data {
        lock.withLock { _signed.append(message) }
        return Self.signature
    }
}

private final class FakeRegistry: InstallsRegistry, @unchecked Sendable {
    private let lock = NSLock()
    private var _teams: [String: [InstallEntry]]
    private var _registered: [RegisterInstallInput] = []
    private var _revoked: [String] = []
    private var _failList = false
    private var _forbidOnce = false

    init(teams: [String: [InstallEntry]]) { _teams = teams }

    var teams: [String: [InstallEntry]] {
        get { lock.withLock { _teams } }
        set { lock.withLock { _teams = newValue } }
    }
    var registered: [RegisterInstallInput] { lock.withLock { _registered } }
    var revoked: [String] { lock.withLock { _revoked } }
    var failList: Bool {
        get { lock.withLock { _failList } }
        set { lock.withLock { _failList = newValue } }
    }
    var forbidOnce: Bool {
        get { lock.withLock { _forbidOnce } }
        set { lock.withLock { _forbidOnce = newValue } }
    }

    func register(accountId: String, input: RegisterInstallInput) async throws -> RegisterInstallResult {
        try lock.withLock {
            if _forbidOnce {
                _forbidOnce = false
                throw TrpcError.httpError(403, #"{"error":{"code":"FORBIDDEN"}}"#)
            }
            _registered.append(input)
            return RegisterInstallResult(installId: "install-\(_registered.count)")
        }
    }

    func list(accountId: String, teamId: String) async throws -> [InstallEntry] {
        try lock.withLock {
            if _failList { throw URLError(.notConnectedToInternet) }
            return _teams[teamId] ?? []
        }
    }

    func revoke(accountId: String, installId: String) async throws {
        lock.withLock { _revoked.append(installId) }
    }
}

final class FakeNode: InstallNodeDriver, @unchecked Sendable {
    let endpointId: String
    private let lock = NSLock()
    private var peers: Set<String> = []
    private var _shutDown = false

    init(endpointId: String) { self.endpointId = endpointId }

    var isShutDown: Bool { lock.withLock { _shutDown } }
    func allowedPeers() -> [String] { lock.withLock { peers.sorted() } }
    func allow(peer: String) throws { _ = lock.withLock { peers.insert(peer) } }
    func revoke(peer: String) throws { _ = lock.withLock { peers.remove(peer) } }
    func shutdown() async { lock.withLock { _shutDown = true } }
}

/// A service over the fakes, recording node starts (by identity) and
/// identity rotations.
private final class Harness: @unchecked Sendable {
    let service: InstallNodeService
    private let lock = NSLock()
    private var identity: Data? = Data(repeating: 7, count: 32)
    private var _started: [Data] = []
    private var _rotations: [Data] = []
    private var _node: FakeNode?

    var started: [Data] { lock.withLock { _started } }
    var rotations: [Data] { lock.withLock { _rotations } }
    var node: FakeNode { lock.withLock { _node! } }

    init(registry: FakeRegistry) {
        var service: InstallNodeService!
        let box = UnsafeBox()
        service = InstallNodeService(
            registry: registry,
            crypto: FakeCrypto(),
            identity: InstallIdentitySource(
                loadOrCreate: { box.harness!.loadOrCreate() },
                rotate: { box.harness!.rotate() }
            ),
            label: "iPhone",
            platform: "iOS 27.1",
            startNode: { seed in box.harness!.start(seed) }
        )
        self.service = service
        box.harness = self
    }

    private func loadOrCreate() -> Data? {
        lock.withLock {
            if identity == nil { identity = Data((0..<32).map { _ in UInt8.random(in: 0...255) }) }
            return identity
        }
    }

    private func rotate() {
        lock.withLock {
            if let identity { _rotations.append(identity) }
            identity = nil
        }
    }

    private func start(_ seed: Data) -> InstallNodeDriver {
        lock.withLock {
            _started.append(seed)
            let node = FakeNode(endpointId: seed == Data(repeating: 7, count: 32) ? "self" : "self-\(_started.count)")
            _node = node
            return node
        }
    }
}

private final class UnsafeBox: @unchecked Sendable {
    weak var harness: Harness?
}
