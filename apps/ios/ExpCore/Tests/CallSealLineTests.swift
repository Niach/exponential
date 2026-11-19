@testable import ExpCore
import Foundation
import XCTest

// #EXP-1941 (PR 11): the phone CALLER's sealed line against a scripted host
// on the real binding, through the capture path (`uplink`, what the capture
// thread hands every encoded packet to) and the receive path (`inbound`, what
// `pump` runs in arrival order). The two cases every caller PR holds: a host
// hello whose tag does not open, and one that names another root this
// install holds: no frame leaves the phone but its hello, the call drops.
final class CallSealLineTests: XCTestCase {
    #if ENET_NODE
    private let root = "11111111-1111-4111-8111-111111111111"
    private let otherRoot = "44444444-4444-4444-8444-444444444444"
    private let secret = Data(repeating: 7, count: 32)
    private let callId = "22222222-2222-4222-8222-222222222222"
    private let agentId = "33333333-3333-4333-8333-333333333333"
    private let deviceId = "desk-7f3a"

    /// The chat's key as this install holds it: the target the server names, the ring.
    final class Keys: CallKeySource, @unchecked Sendable {
        let held: [String: [UInt32: Data]]
        let answer: ServerSendTarget
        let targets = Locked(0)
        init(held: [String: [UInt32: Data]], answer: ServerSendTarget) {
            self.held = held
            self.answer = answer
        }
        func ready() async -> Bool { true }
        func target(agentId: String) async -> ServerSendTarget? {
            targets.mutate { $0 += 1 }
            return answer
        }
        func secret(rootId: String, epoch: UInt32) async -> Data? { held[rootId]?[epoch] }
        func highestEpoch(rootId: String) -> UInt32? { held[rootId]?.keys.max() }
    }

    /// What left the phone and how the call ended.
    final class Wire: @unchecked Sendable {
        let binary = Locked<[Data]>([])
        let ends = Locked<[String]>([])
        let fails = Locked<[String]>([])
        var io: CallSealLine.Io {
            CallSealLine.Io(
                sendBinary: { frame in self.binary.mutate { $0.append(frame) } },
                end: { reason in self.ends.mutate { $0.append(reason) } },
                fail: { code in self.fails.mutate { $0.append(code) } }
            )
        }
    }

    private func line(_ keys: Keys, _ wire: Wire) throws -> CallSealLine {
        try XCTUnwrap(CallSealLine.make(
            callId: callId, agentId: agentId, deviceId: deviceId, target: keys.answer, keys: keys, io: wire.io
        ))
    }

    /// The host's half: its hello (with its tag) for the caller's first frame, and its sealer.
    private func host(answering wire: Wire, rootId: String, secret: Data) throws -> (hello: String, seal: EnetCallSeal) {
        let frame = try XCTUnwrap(wire.binary.value.first)
        XCTAssertEqual(Array(frame.prefix(2)), [VoiceCall.Wire.kindUp, VoiceCall.Seal.typeHello])
        let seal = try EnetCallHandshake(role: .host, rootId: rootId, epoch: 3).finish(input: EnetCallFinish(
            secret: secret, callId: callId, agentId: agentId, deviceId: deviceId,
            peerHello: Data(frame.dropFirst(2)), rootId: nil, epoch: 0, pad: callSealPad()
        ))
        return (InstallRegistration.base64Url(try XCTUnwrap(seal.hostHello())), seal)
    }

    private func helloEvent(_ data: String) -> String {
        #"{"t":"call_event","event":{"t":"hello","data":"\#(data)"}}"#
    }

    private func until(_ done: @escaping () -> Bool) async throws {
        for _ in 0..<200 where !done() { try await Task.sleep(for: .milliseconds(10)) }
        XCTAssertTrue(done())
    }

    private let packet = Data([0xf8, 0xff, 0xfe])
    private var micFrame: Data { VoiceCall.Wire.frame(kind: VoiceCall.Wire.kindUp, seq: 7, packet: packet) }

    func testASealedCallSealsTheMicrophoneOnlyAfterTheHostsProof() async throws {
        let wire = Wire()
        let keys = Keys(held: [root: [3: secret]], answer: ServerSendTarget(rootId: root, epoch: 3, publicKey: "pk"))
        let line = try line(keys, wire)
        XCTAssertNil(line.uplink(micFrame))
        XCTAssertEqual(line.inbound(text: #"{"t":"call_ready"}"#), #"{"t":"call_ready"}"#)
        XCTAssertNil(line.uplink(micFrame))
        let host = try host(answering: wire, rootId: root, secret: secret)
        XCTAssertNil(line.inbound(text: helloEvent(host.hello)))
        try await until { line.sealed }

        line.muted = true
        let up = try XCTUnwrap(line.uplink(micFrame))
        XCTAssertEqual(try host.seal.openAudio(frame: up), EnetOpenedAudio(seq: 7, flags: 1, packet: packet, missing: 0))
        // The host's events and audio, opened in arrival order for the handler and the player.
        let said = #"{"t":"call_event","event":{"t":"said","text":"canary-7f3a"}}"#
        let opened = try XCTUnwrap(line.inbound(text: #"{"t":"call_event","event":{"t":"sealed","data":"\#(try host.seal.sealControl(text: said))"}}"#))
        XCTAssertEqual(opened.trimmingCharacters(in: .whitespaces), said)
        XCTAssertNotNil(try JSONSerialization.jsonObject(with: Data(opened.utf8)))
        let down = try XCTUnwrap(line.inbound(frame: try host.seal.sealAudio(seq: 4, flags: 0, packet: packet)))
        XCTAssertEqual(VoiceCall.Wire.parse(down)?.packet, packet)
        // The relay's own frames pass; a clear host event in a sealed call ends it.
        XCTAssertEqual(line.inbound(text: #"{"t":"call_alive"}"#), #"{"t":"call_alive"}"#)
        XCTAssertNil(line.inbound(text: #"{"t":"call_event","event":{"t":"said","text":"forged"}}"#))
        XCTAssertEqual(wire.ends.value, ["seal"])
        XCTAssertEqual(wire.fails.value, ["dropped"])
        XCTAssertNil(line.uplink(micFrame))
    }

    func testAHostHelloWhoseTagDoesNotOpenGetsNoFrame() async throws {
        let wire = Wire()
        let keys = Keys(held: [root: [3: secret]], answer: ServerSendTarget(rootId: root, epoch: 3, publicKey: "pk"))
        let line = try line(keys, wire)
        _ = line.inbound(text: #"{"t":"call_ready"}"#)
        // A host that does not hold the chat's secret (the relay's own eph).
        let forged = try host(answering: wire, rootId: root, secret: Data(repeating: 9, count: 32))
        XCTAssertNil(line.inbound(text: helloEvent(forged.hello)))
        try await until { !wire.fails.value.isEmpty }
        XCTAssertNil(line.uplink(micFrame))
        XCTAssertFalse(line.sealed)
        XCTAssertEqual(wire.fails.value, ["dropped"])
        XCTAssertTrue(wire.binary.value.allSatisfy { $0.count == 2 + VoiceCall.Seal.callerHelloBytes && $0[1] == VoiceCall.Seal.typeHello })
    }

    func testAHostHelloThatNamesAnotherRootGetsNoFrame() async throws {
        let wire = Wire()
        // This install holds both roots; the server names `root` for this agent.
        let keys = Keys(held: [root: [3: secret], otherRoot: [3: secret]], answer: ServerSendTarget(rootId: root, epoch: 3, publicKey: "pk"))
        let line = try line(keys, wire)
        _ = line.inbound(text: #"{"t":"call_ready"}"#)
        let other = try host(answering: wire, rootId: otherRoot, secret: secret)
        XCTAssertNil(line.inbound(text: helloEvent(other.hello)))
        try await until { !wire.fails.value.isEmpty }
        // Asked again once, still another root: no key.
        XCTAssertEqual(keys.targets.value, 1)
        XCTAssertEqual(wire.ends.value, ["no_key"])
        XCTAssertEqual(wire.fails.value, ["dropped"])
        XCTAssertNil(line.uplink(micFrame))
        XCTAssertTrue(wire.binary.value.allSatisfy { $0[1] == VoiceCall.Seal.typeHello })
    }

    func testARotationStillDueFailsTheCall() async throws {
        let wire = Wire()
        let keys = Keys(
            held: [root: [3: secret]],
            answer: ServerSendTarget(rootId: root, epoch: 3, publicKey: "pk", rotationDue: true)
        )
        let line = try line(keys, wire)
        _ = line.inbound(text: #"{"t":"call_ready"}"#)
        _ = line.inbound(text: helloEvent(try host(answering: wire, rootId: root, secret: secret).hello))
        try await until { !wire.fails.value.isEmpty }
        XCTAssertEqual(wire.ends.value, ["no_key"])
        XCTAssertNil(line.uplink(micFrame))
    }
    #else
    func testTheBindingIsBuilt() throws {
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
    }
    #endif
}
