@testable import ExpCore
import Foundation
import XCTest

// #EXP-1941: packages/etalk/fixtures/voice-call.json `seal` through the FULL
// eNet binding. No fixed eph secret crosses the binding, so the vectors
// replay inside Rust (`callSealSelftest`); one live round trip goes through
// the two objects. The same replay runs in TS, Rust and Kotlin.
final class CallSealFixtureTests: XCTestCase {
    #if ENET_NODE
    private let root = "11111111-1111-4111-8111-111111111111"

    private func fixture() throws -> String {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/etalk/fixtures/voice-call.json")
        return try String(contentsOf: url, encoding: .utf8)
    }

    private func finish(_ peerHello: Data, rootId: String? = nil) -> EnetCallFinish {
        EnetCallFinish(
            secret: Data(repeating: 7, count: 32),
            callId: "22222222-2222-4222-8222-222222222222",
            agentId: "33333333-3333-4333-8333-333333333333",
            deviceId: "desk-7f3a",
            peerHello: peerHello,
            rootId: rootId ?? root,
            epoch: 3,
            pad: callSealPad()
        )
    }

    /// A caller that knew no root, and the host that answered its hello.
    private func call() throws -> (EnetCallHandshake, EnetCallSeal, Data) {
        let caller = try EnetCallHandshake(role: .caller, rootId: nil, epoch: 0)
        let host = try EnetCallHandshake(role: .host, rootId: root, epoch: 3).finish(input: finish(try caller.hello()))
        return (caller, host, try XCTUnwrap(host.hostHello()))
    }

    private func reason(_ run: () throws -> Void) -> String {
        do {
            try run()
            return "accepted"
        } catch EnetError.Crypto(let reason) {
            return reason
        } catch {
            return "\(error)"
        }
    }

    func testTheSelftestReplaysTheFixtureAndNamesWhatFails() throws {
        let fixture = try fixture()
        try callSealSelftest(fixtureJson: fixture)
        let broken = fixture.replacingOccurrences(of: "021df2d28ad2384e35f042fbb30b425d", with: "031df2d28ad2384e35f042fbb30b425d")
        let failed = reason { try callSealSelftest(fixtureJson: broken) }
        XCTAssertTrue(failed.contains("hostHelloTag"), failed)
    }

    func testALiveRoundTripThroughTheTwoObjects() throws {
        let (callerSide, host, hostHello) = try call()
        XCTAssertEqual([try callerSide.hello().count, hostHello.count], [54, 70])
        // A host seals nothing before a caller frame opened.
        XCTAssertEqual(reason { _ = try host.sealAudio(seq: 0, flags: 0, packet: Data([1, 2, 3])) }, "call seal: unproven")
        XCTAssertEqual(reason { _ = try host.sealControl(text: "{}") }, "call seal: unproven")
        let caller = try callerSide.finish(input: finish(hostHello))
        XCTAssertEqual(reason { _ = try callerSide.finish(input: self.finish(hostHello)) }, "call seal: finished")

        let packet = Data([0xf8, 0xff, 0xfe])
        let up = try caller.sealAudio(seq: 9, flags: 1, packet: packet)
        XCTAssertEqual(up.count, 27 + Int(callSealPad().audioUp))
        XCTAssertEqual(try host.openAudio(frame: up), EnetOpenedAudio(seq: 9, flags: 1, packet: packet, missing: 0))
        let caption = #"{"t":"call_event","event":{"t":"said","text":"hello"}}"#
        let opened = try caller.openControl(data: try host.sealControl(text: caption))
        XCTAssertEqual(opened.trimmingCharacters(in: .whitespaces), caption)
        // A packet above the step is refused; the empty frame in its place is one missing.
        XCTAssertEqual(reason { _ = try caller.sealAudio(seq: 10, flags: 0, packet: Data(count: 121)) }, "call seal: too_large")
        XCTAssertEqual(try host.openAudio(frame: try caller.sealAudio(seq: 10, flags: 0, packet: Data())).missing, 1)
    }

    func testAHostHelloWithAFlippedTagBitIsRefusedForGood() throws {
        let (caller, _, hostHello) = try call()
        var flipped = hostHello
        flipped[flipped.startIndex + 69] ^= 1
        XCTAssertEqual(reason { _ = try caller.finish(input: self.finish(flipped)) }, "call seal: tag")
        XCTAssertEqual(reason { _ = try caller.finish(input: self.finish(hostHello)) }, "call seal: finished")
        let (other, _, otherHello) = try call()
        XCTAssertEqual(reason { _ = try other.finish(input: self.finish(otherHello, rootId: "11111111-1111-4111-8111-111111111112")) }, "call seal: root")
    }
    #else
    func testTheBindingIsBuilt() throws {
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
    }
    #endif
}
