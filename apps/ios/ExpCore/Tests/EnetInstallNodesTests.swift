@testable import ExpCore
import CryptoKit
import Foundation
import XCTest

// EXP-1457 step 1 acceptance on REAL eNet nodes (the full binding, relays
// off, loopback): two installs of the same team load each other from the
// registry and exchange an acked chat ping; an install of another team is
// refused. Plus the registration signature checked as real ed25519.
#if ENET_NODE
final class EnetInstallNodesTests: XCTestCase {
    func testTeamInstallsExchangeAnAckedPingAndAForeignEndpointIsRefused() async throws {
        let crypto = EnetInstallCrypto()
        let a = generateIdentity(), b = generateIdentity(), foreign = generateIdentity()
        let registry = MemoryRegistry(teams: [
            "team": [try install(a, crypto), try install(b, crypto)],
            "other": [try install(foreign, crypto)],
        ])
        let started = StartedNodes()
        func service(_ seed: Data) -> InstallNodeService {
            InstallNodeService(
                registry: registry, crypto: crypto,
                identity: InstallIdentitySource(loadOrCreate: { seed }, rotate: {}),
                label: "test", platform: "iOS",
                startNode: { seed in
                    let node = try await EnetInstallNode.start(config: EnetInstallNode.localOnlyConfig(), identity: seed)
                    started.add(node)
                    return node
                }
            )
        }
        let serviceA = service(a), serviceB = service(b)
        await serviceA.sync([InstallAccount(accountId: "acc-a", userId: "user-a", teamIds: ["team"])])
        await serviceB.sync([InstallAccount(accountId: "acc-b", userId: "user-b", teamIds: ["team"])])
        XCTAssertEqual(registry.registeredCount, 2)
        let nodeA = try XCTUnwrap(started.node(try crypto.endpointId(identity: a)))
        let nodeB = try XCTUnwrap(started.node(try crypto.endpointId(identity: b)))
        XCTAssertEqual(nodeA.allowedPeers(), [nodeB.endpointId])
        XCTAssertEqual(nodeB.allowedPeers(), [nodeA.endpointId])

        // Same team: B pings A, A acks (sendChat returns once acknowledged).
        _ = try nodeB.node.addTicket(ticket: nodeA.node.ticket())
        // A's service pump owns nextEvent (VAPP-33): non-steer chat reaches its tap.
        var eventsA = serviceA.otherEvents.makeAsyncIterator()
        _ = try await nodeB.node.sendChat(peer: nodeA.endpointId, body: Data("ping".utf8))
        guard case let .chat(from, body)? = await eventsA.next() else {
            return XCTFail("A got no chat event")
        }
        XCTAssertEqual(from, nodeB.endpointId)
        XCTAssertEqual(body, Data("ping".utf8))

        // Another team's install: A refuses it.
        let outsider = try await EnetInstallNode.start(config: EnetInstallNode.localOnlyConfig(), identity: foreign)
        _ = try outsider.node.addTicket(ticket: nodeA.node.ticket())
        do {
            _ = try await outsider.node.sendChat(peer: nodeA.endpointId, body: Data("let me in".utf8))
            XCTFail("a foreign endpoint got through")
        } catch let error as EnetError {
            switch error {
            case .Refused, .Connect, .Transport: break
            default: XCTFail("unexpected error \(error)")
            }
        }
        guard case let .peerRefused(peer)? = await eventsA.next() else {
            return XCTFail("A did not report the refused peer")
        }
        XCTAssertEqual(peer, outsider.endpointId)

        await outsider.shutdown()
        await serviceA.stop()
        await serviceB.stop()
    }

    /// VAPP-33 on REAL nodes: the viewer (an InstallNodeService + its steer
    /// carrier) joins a device over the chat channel, gets the replay, a live
    /// frame, and steers an answer back; a stranger's envelope for the same
    /// session is ignored.
    func testViewerJoinsTheDeviceOverTheChatChannel() async throws {
        let crypto = EnetInstallCrypto()
        let viewerSeed = generateIdentity(), deviceSeed = generateIdentity(), strangerSeed = generateIdentity()
        let registry = MemoryRegistry(teams: [
            "team": [try install(viewerSeed, crypto), try install(deviceSeed, crypto), try install(strangerSeed, crypto)],
        ])
        let started = StartedNodes()
        let viewer = InstallNodeService(
            registry: registry, crypto: crypto,
            identity: InstallIdentitySource(loadOrCreate: { viewerSeed }, rotate: {}),
            label: "test", platform: "iOS",
            startNode: { seed in
                let node = try await EnetInstallNode.start(config: EnetInstallNode.localOnlyConfig(), identity: seed)
                started.add(node)
                return node
            }
        )
        await viewer.sync([InstallAccount(accountId: "acc", userId: "u", teamIds: ["team"])])
        let viewerNode = try XCTUnwrap(started.node(try crypto.endpointId(identity: viewerSeed)))
        let device = try await EnetInstallNode.start(config: EnetInstallNode.localOnlyConfig(), identity: deviceSeed)
        let stranger = try await EnetInstallNode.start(config: EnetInstallNode.localOnlyConfig(), identity: strangerSeed)
        for node in [device, stranger] {
            try node.allow(peer: viewerNode.endpointId)
            _ = try node.node.addTicket(ticket: viewerNode.node.ticket())
        }
        _ = try viewerNode.node.addTicket(ticket: device.node.ticket())

        // The device: answers `join` with the replay, then records what it is sent.
        let received = Received()
        let deviceTask = Task.detached {
            while let event = await device.nextEvent() {
                guard case let .chat(from, body) = event, let envelope = SteerEnet.parse(body) else { continue }
                received.add(envelope.types)
                if envelope.types.contains("join") {
                    _ = try? await device.sendChat(peer: from, body: SteerEnet.encode(sessionId: envelope.sessionId, frames: [
                        #"{"t":"activity_reset"}"#,
                        #"{"t":"activity","seq":0,"event":{"kind":"narration","text":"hi"}}"#,
                        #"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#,
                    ]))
                }
            }
        }

        let carrier = InstallSteerCarrier(service: viewer, lookup: FixedLookup(endpoints: [device.endpointId]))
        let outcome = await SteerEnetJoiner.join(carrier: carrier, accountId: "acc", sessionId: "s1", timeout: .seconds(10))
        guard case let .joined(link, replay) = outcome else { return XCTFail("not joined: \(outcome)") }
        XCTAssertEqual(replay.compactMap(SteerEnet.frameType), ["activity_reset", "activity", "activity_synced"])
        var frames = link.stream(onSendFailure: {}).makeAsyncIterator()

        // A stranger in the team sends for the same session: not applied.
        _ = try await stranger.sendChat(peer: viewerNode.endpointId, body: SteerEnet.encode(sessionId: "s1", frames: [#"{"t":"bye","outcome":"killed"}"#]))
        _ = try await device.sendChat(peer: viewerNode.endpointId, body: SteerEnet.encode(sessionId: "s1", frames: [#"{"t":"keepalive"}"#]))
        let live = await frames.next()
        XCTAssertEqual(live?.compactMap(SteerEnet.frameType), ["keepalive"])

        link.send([#"{"t":"answer","questionId":"q1","keys":["yes"]}"#])
        for _ in 0..<300 where !received.all.contains(["answer"]) {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertEqual(received.all, [["join"], ["answer"]])

        link.close()
        deviceTask.cancel()
        await device.shutdown()
        await stranger.shutdown()
        await viewer.stop()
    }

    /// The signature the registry checks: ed25519 by the endpoint id's key over
    /// the contract string (verified independently with CryptoKit).
    func testRegistrationSignatureVerifiesAgainstTheEndpointId() throws {
        let crypto = EnetInstallCrypto()
        let identity = generateIdentity()
        let input = try InstallRegistration.input(
            identity: identity, userId: "user-1", crypto: crypto,
            label: nil, platform: nil, now: Date()
        )
        XCTAssertNotNil(input.endpointId.range(of: "^[0-9a-f]{64}$", options: .regularExpression))
        XCTAssertEqual(input.x25519Public.count, 43)
        XCTAssertEqual(input.signature.count, 86)
        let key = try Curve25519.Signing.PublicKey(rawRepresentation: hexBytes(input.endpointId))
        let message = InstallRegistration.message(
            userId: "user-1", endpointId: input.endpointId, x25519Public: input.x25519Public, signedAt: input.signedAt
        )
        XCTAssertTrue(key.isValidSignature(try XCTUnwrap(base64UrlBytes(input.signature)), for: Data(message.utf8)))
        XCTAssertFalse(key.isValidSignature(try XCTUnwrap(base64UrlBytes(input.signature)), for: Data((message + "x").utf8)))
        // X25519 = the push key derivation.
        XCTAssertEqual(input.x25519Public, InstallRegistration.base64Url(try pushKeypairDerive(identity: identity).public))
    }

    /// `packages/domain-contract/fixtures/install-register.json`: the shared
    /// vector every platform signs byte-for-byte (ed25519 is deterministic).
    func testSignatureMatchesTheSharedFixture() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/install-register.json")
        let fixture = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let identity = try hexBytes(try XCTUnwrap(fixture["identity"] as? String))
        let userId = try XCTUnwrap(fixture["userId"] as? String)
        let input = try InstallRegistration.input(
            identity: identity,
            userId: userId,
            crypto: EnetInstallCrypto(),
            label: nil, platform: nil,
            now: try XCTUnwrap(ISO8601DateFormatter.withMillis.date(from: try XCTUnwrap(fixture["signedAt"] as? String)))
        )
        XCTAssertEqual(input.endpointId, fixture["endpointId"] as? String)
        XCTAssertEqual(input.x25519Public, fixture["x25519Public"] as? String)
        XCTAssertEqual(input.signedAt, fixture["signedAt"] as? String)
        XCTAssertEqual(
            InstallRegistration.message(userId: userId, endpointId: input.endpointId, x25519Public: input.x25519Public, signedAt: input.signedAt),
            fixture["message"] as? String
        )
        XCTAssertEqual(try identitySign(identity: identity, message: Data(try XCTUnwrap(fixture["message"] as? String).utf8)),
                       try hexBytes(try XCTUnwrap(fixture["signatureHex"] as? String)))
        XCTAssertEqual(input.signature, fixture["signature"] as? String)
    }
}

private extension ISO8601DateFormatter {
    static var withMillis: ISO8601DateFormatter {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }
}

private func hexBytes(_ hex: String) throws -> Data {
    var bytes = [UInt8]()
    var index = hex.startIndex
    while index < hex.endIndex {
        let next = hex.index(index, offsetBy: 2)
        bytes.append(try XCTUnwrap(UInt8(hex[index..<next], radix: 16)))
        index = next
    }
    return Data(bytes)
}

private func base64UrlBytes(_ s: String) -> Data? {
    var b = s.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    while b.count % 4 != 0 { b += "=" }
    return Data(base64Encoded: b)
}

private func install(_ identity: Data, _ crypto: EnetInstallCrypto) throws -> InstallEntry {
    InstallEntry(
        installId: UUID().uuidString, userId: "u", kind: "ios",
        endpointId: try crypto.endpointId(identity: identity),
        x25519Public: InstallRegistration.base64Url(try crypto.x25519Public(identity: identity))
    )
}

private final class StartedNodes: @unchecked Sendable {
    private let lock = NSLock()
    private var nodes: [EnetInstallNode] = []
    func add(_ node: EnetInstallNode) { lock.withLock { nodes.append(node) } }
    func node(_ endpointId: String) -> EnetInstallNode? { lock.withLock { nodes.first { $0.endpointId == endpointId } } }
}

private final class MemoryRegistry: InstallsRegistry, @unchecked Sendable {
    private let lock = NSLock()
    private let teams: [String: [InstallEntry]]
    private var registered = 0
    init(teams: [String: [InstallEntry]]) { self.teams = teams }
    var registeredCount: Int { lock.withLock { registered } }

    func register(accountId: String, input: RegisterInstallInput) async throws -> RegisterInstallResult {
        lock.withLock { registered += 1 }
        return RegisterInstallResult(installId: UUID().uuidString)
    }
    func list(accountId: String, teamId: String) async throws -> [InstallEntry] { teams[teamId] ?? [] }
    func revoke(accountId: String, installId: String) async throws {}
}
#endif

private struct FixedLookup: SteerEndpointLookup {
    let endpoints: [String]
    func forSession(accountId: String, sessionId: String) async throws -> [SteerEndpoint] {
        endpoints.map { SteerEndpoint(endpointId: $0, kind: "desktop") }
    }
}

private final class Received: @unchecked Sendable {
    private let lock = NSLock()
    private var types: [[String]] = []
    func add(_ value: [String]) { lock.withLock { types.append(value) } }
    var all: [[String]] { lock.withLock { types } }
}
