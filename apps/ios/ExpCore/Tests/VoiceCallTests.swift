import Foundation
import XCTest
@testable import ExpCore

// NEW-69: caller mode, locked ×4 (TS `voice-call.ts`, desktop
// `domain::voice_call`, Android `VoiceCallTest`) against
// fixtures/voice-call.json: every `copy` key (incl. `reasons` and
// `failed`), the `testIds`, `icons`, `constants`, `wire`,
// `surfaceSpecimens`, and every `availabilityCases` and `flowCases` row
// (view + toast).
final class VoiceCallTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/voice-call.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(cases.isEmpty, key)
        return cases
    }

    func testCopyMatchesTheFixture() throws {
        var copy = try XCTUnwrap(try fixture()["copy"] as? [String: Any])
        let reasons = try XCTUnwrap(copy.removeValue(forKey: "reasons") as? [String: String])
        let failed = try XCTUnwrap(copy.removeValue(forKey: "failed") as? [String: String])
        let greeting = try XCTUnwrap(copy.removeValue(forKey: "greeting") as? [String: String])
        XCTAssertEqual(copy as? [String: String], [
            "call": VoiceCall.Copy.call,
            "calling": VoiceCall.Copy.calling,
            "gettingReady": VoiceCall.Copy.gettingReady,
            "noSound": VoiceCall.Copy.noSound,
            "listening": VoiceCall.Copy.listening,
            "thinking": VoiceCall.Copy.thinking,
            "speaking": VoiceCall.Copy.speaking,
            "muted": VoiceCall.Copy.muted,
            "mute": VoiceCall.Copy.mute,
            "unmute": VoiceCall.Copy.unmute,
            "speaker": VoiceCall.Copy.speaker,
            "speakerOff": VoiceCall.Copy.speakerOff,
            "endCall": VoiceCall.Copy.endCall,
            "cancel": VoiceCall.Copy.cancel,
            "queued": VoiceCall.Copy.queued,
        ])
        XCTAssertEqual(greeting, VoiceCall.Copy.greeting)
        XCTAssertEqual(VoiceCall.callGreeting("de"), "Hallo?")
        XCTAssertEqual(VoiceCall.callGreeting("fr"), "Hello?")
        XCTAssertEqual(VoiceCall.callGreeting(nil), "Hello?")
        XCTAssertEqual(reasons, [
            "noVoiceDevice": VoiceCall.Copy.Reasons.noVoiceDevice,
            "offline": VoiceCall.Copy.Reasons.offline,
            "notHere": VoiceCall.Copy.Reasons.notHere,
            "preparing": VoiceCall.Copy.Reasons.preparing,
            "noGpu": VoiceCall.Copy.Reasons.noGpu,
            "busy": VoiceCall.Copy.Reasons.busy,
            "meeting": VoiceCall.Copy.Reasons.meeting,
        ])
        XCTAssertEqual(failed, VoiceCall.Copy.failed)
        XCTAssertEqual(VoiceCall.Copy.call(agent: "Ada"), "Call Ada")
    }

    func testTestIdsIconsConstantsAndWireMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(try XCTUnwrap(fixture["testIds"] as? [String: String]), [
            "call": VoiceCall.TestIds.call,
            "surface": VoiceCall.TestIds.surface,
            "mute": VoiceCall.TestIds.mute,
            "end": VoiceCall.TestIds.end,
            "speaker": VoiceCall.TestIds.speaker,
            "panel": VoiceCall.TestIds.panel,
            "pill": VoiceCall.TestIds.pill,
            "timer": VoiceCall.TestIds.timer,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["icons"] as? [String: String]), [
            "call": VoiceCall.Icons.call,
            "mute": VoiceCall.Icons.mute,
            "unmute": VoiceCall.Icons.unmute,
            "speaker": VoiceCall.Icons.speaker,
            "speakerOff": VoiceCall.Icons.speakerOff,
            "end": VoiceCall.Icons.end,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["pill"] as? [String: Int]), [
            "width": VoiceCall.Pill.width,
            "height": VoiceCall.Pill.height,
            "marginPx": VoiceCall.Pill.marginPx,
        ])
        var constants = try XCTUnwrap(fixture["constants"] as? [String: NSNumber])
        XCTAssertEqual(constants.removeValue(forKey: "noSoundLevel")?.doubleValue, VoiceCall.Constants.noSoundLevel)
        XCTAssertEqual(constants.mapValues(\.intValue), [
            "ringMs": VoiceCall.Constants.ringMs,
            "maxRingMs": VoiceCall.Constants.maxRingMs,
            "ringTimeoutMs": VoiceCall.Constants.ringTimeoutMs,
            "minSilenceMs": VoiceCall.Constants.minSilenceMs,
            "bargeInMs": VoiceCall.Constants.bargeInMs,
            "frameMs": VoiceCall.Constants.frameMs,
            "upRate": VoiceCall.Constants.upRate,
            "downRate": VoiceCall.Constants.downRate,
            "upKbps": VoiceCall.Constants.upKbps,
            "downKbps": VoiceCall.Constants.downKbps,
            "jitterMs": VoiceCall.Constants.jitterMs,
            "maxFramesPerSecond": VoiceCall.Constants.maxFramesPerSecond,
            "maxCallMs": VoiceCall.Constants.maxCallMs,
            "idleTimeoutMs": VoiceCall.Constants.idleTimeoutMs,
        ])
        let wire = try XCTUnwrap(fixture["wire"] as? [String: Any])
        XCTAssertEqual(try XCTUnwrap(wire["kinds"] as? [String: Int]), [
            "up": Int(VoiceCall.Wire.kindUp),
            "down": Int(VoiceCall.Wire.kindDown),
        ])
        XCTAssertEqual(wire["maxFrameBytes"] as? Int, VoiceCall.Wire.maxFrameBytes)
    }

    func testFramesRoundTrip() {
        let frame = VoiceCall.Wire.frame(kind: VoiceCall.Wire.kindUp, seq: 0xfffe, packet: Data([7, 8, 9]))
        XCTAssertEqual([UInt8](frame), [1, 0xff, 0xfe, 7, 8, 9])
        let parsed = VoiceCall.Wire.parse(frame)
        XCTAssertEqual(parsed?.kind, VoiceCall.Wire.kindUp)
        XCTAssertEqual(parsed?.seq, 0xfffe)
        XCTAssertEqual(parsed?.packet, Data([7, 8, 9]))
        XCTAssertNil(VoiceCall.Wire.parse(Data([2, 0, 0])))
        XCTAssertNil(VoiceCall.Wire.parse(Data(repeating: 2, count: VoiceCall.Wire.maxFrameBytes + 1)))
    }

    func testSurfaceSpecimensMatchTheFixture() throws {
        let rows = try cases("surfaceSpecimens").map { row in
            VoiceCall.Specimen(
                name: row["name"] as? String ?? "",
                agent: row["agent"] as? String ?? "",
                timer: row["timer"] as? String,
                level: (row["level"] as? NSNumber)?.doubleValue ?? -1,
                bands: (row["bands"] as? [NSNumber])?.map(\.doubleValue) ?? [-1],
                view: try Self.view(row)
            )
        }
        XCTAssertEqual(rows, VoiceCall.surfaceSpecimens)
    }

    func testAvailabilityCases() throws {
        for row in try cases("availabilityCases") {
            let name = row["name"] as? String ?? "?"
            let input = try XCTUnwrap(row["input"] as? [String: Any], name)
            let expected = try XCTUnwrap(row["expected"] as? [String: Any], name)
            let speech = input["speech"] as? String
            let availability = VoiceCall.callAvailability(VoiceCall.AvailabilityInput(
                platform: try XCTUnwrap(VoiceCall.Platform(rawValue: input["platform"] as? String ?? ""), name),
                device: input["device"] as? String,
                online: input["online"] as? Bool ?? false,
                here: input["here"] as? Bool ?? false,
                speech: try speech.map { try XCTUnwrap(VoiceCall.Speech(rawValue: $0), name) },
                busy: input["busy"] as? Bool ?? false,
                meeting: input["meeting"] as? Bool ?? false
            ))
            XCTAssertEqual(availability, VoiceCall.Availability(
                enabled: expected["enabled"] as? Bool ?? false,
                reason: expected["reason"] as? String
            ), name)
        }
    }

    func testFlowCases() throws {
        for row in try cases("flowCases") {
            let name = row["name"] as? String ?? "?"
            let events = try XCTUnwrap(row["events"] as? [[String: Any]], name).map {
                try XCTUnwrap(VoiceCall.Event(json: $0), "\(name): \($0)")
            }
            let state = events.reduce(VoiceCall.idle, VoiceCall.callStep)
            let platform = try XCTUnwrap(VoiceCall.Platform(rawValue: row["platform"] as? String ?? ""), name)
            let view = VoiceCall.callView(state, platform: platform)
            if let expected = row["view"] as? [String: Any] {
                XCTAssertEqual(view, try Self.view(expected), name)
            } else {
                XCTAssertNil(view, name)
            }
            XCTAssertEqual(state.toast, row["toast"] as? String, name)
        }
    }

    func testTimerCases() throws {
        for row in try cases("timerCases") {
            let ms = try XCTUnwrap(row["ms"] as? Int)
            XCTAssertEqual(VoiceCall.callTimer(ms: ms), row["text"] as? String, "\(ms)")
        }
    }

    /// A fixture `view` object (a `flowCases` view or a specimen row).
    private static func view(_ json: [String: Any]) throws -> VoiceCall.View {
        VoiceCall.View(
            activity: try XCTUnwrap(VoiceCall.Activity(rawValue: json["activity"] as? String ?? "")),
            status: json["status"] as? String,
            caption: try XCTUnwrap(json["caption"] as? String),
            muted: try XCTUnwrap(json["muted"] as? Bool),
            speaker: try XCTUnwrap(json["speaker"] as? Bool),
            buttons: try XCTUnwrap(json["buttons"] as? [String]).map { try XCTUnwrap(VoiceCall.Button(rawValue: $0)) },
            end: try XCTUnwrap(json["end"] as? String)
        )
    }

    /// An unknown failure code (the relay's `no_such_call`) reads as
    /// `dropped`, like the TS reference.
    func testAnUnknownFailureReadsAsDropped() {
        let state = [VoiceCall.Event.start(device: "mint"), .failed(code: "no_such_call")]
            .reduce(VoiceCall.idle, VoiceCall.callStep)
        XCTAssertEqual(state.toast, "The call dropped")
        XCTAssertNil(VoiceCall.callView(state))
    }

    /// The relay's `busy` error and its `max_length` / `idle` `call_end`s
    /// end a running call with their `copy.failed` toast.
    func testRelayEndsToast() {
        func ended(_ last: VoiceCall.Event) -> VoiceCall.State {
            [VoiceCall.Event.start(device: "mint"), .rang, .ready, .replyFinished, last].reduce(VoiceCall.idle, VoiceCall.callStep)
        }
        XCTAssertEqual(ended(.failed(code: "busy")).toast, "mint is in another call")
        // hangUpRules: ANY reason (one this build doesn't know too) drops.
        for reason in ["max_length", "idle", "dropped", "hangup"] {
            let state = ended(.hungUp(reason: reason))
            XCTAssertEqual(state.toast, "The call dropped", reason)
            XCTAssertNil(VoiceCall.callView(state), reason)
        }
        for reason in [nil, ""] as [String?] {
            let state = ended(.hungUp(reason: reason))
            XCTAssertNil(state.toast)
            XCTAssertNil(VoiceCall.callView(state))
        }
    }

    /// One frame in flight, `capacity` waiting; a stall drops the oldest.
    func testOutboxDropsTheOldestWhenStalled() {
        var outbox = VoiceCall.Outbox<Int>()
        XCTAssertEqual(outbox.offer(0), 0)
        let capacity = VoiceCall.Outbox<Int>.capacity
        XCTAssertEqual(capacity, 25)
        for frame in 1...(capacity + 5) { XCTAssertNil(outbox.offer(frame)) }
        XCTAssertEqual(outbox.waiting.count, capacity)
        XCTAssertEqual(outbox.dropped, 5)
        XCTAssertEqual(outbox.sent(), 6)
        var last = 6
        while let next = outbox.sent() { last = next }
        XCTAssertEqual(last, capacity + 5)
        XCTAssertFalse(outbox.inFlight)
        XCTAssertEqual(outbox.offer(99), 99)
    }

    /// Muted, the uplink keeps one frame per `frameMs`, all silence, so the
    /// relay's `idleTimeoutMs` never fires; unmuted, the samples go out as
    /// captured.
    func testMutedUplinkKeepsTheCallAlive() {
        let perFrame = VoiceCall.Uplink.frameSamples
        XCTAssertEqual(perFrame, 320)
        var uplink = VoiceCall.Uplink()
        let speech = [Int16](repeating: 1200, count: perFrame * 3 / 2)
        XCTAssertEqual(uplink.push(speech), [Array(speech.prefix(perFrame))])

        uplink.setMuted(true)
        // A whole idle window of muted capture, in 10 ms buffers.
        let buffers = VoiceCall.Constants.idleTimeoutMs / 10
        var frames: [[Int16]] = []
        for _ in 0..<buffers {
            frames += uplink.push([Int16](repeating: 900, count: perFrame / 2))
        }
        XCTAssertEqual(frames.count, VoiceCall.Constants.idleTimeoutMs / VoiceCall.Constants.frameMs)
        XCTAssertTrue(frames.allSatisfy { $0.count == perFrame && $0.allSatisfy { $0 == 0 } })
        // Never a gap close to the idle timeout.
        XCTAssertLessThan(VoiceCall.Constants.frameMs * 2, VoiceCall.Constants.idleTimeoutMs)

        uplink.setMuted(false)
        XCTAssertEqual(uplink.push(speech.prefix(perFrame)), [Array(speech.prefix(perFrame))])
    }

    /// Only the six flow events ride a `call_event` frame.
    /// `hangUpRules`: the fixture's rule is the phone's.
    func testHangUpRulesMatchTheFixture() throws {
        XCTAssertEqual(try XCTUnwrap(try fixture()["hangUpRules"] as? [String: String]), [
            "withReason": "failed",
            "failedCode": VoiceCall.HangUpRules.failedCode,
            "withoutReason": "end",
        ])
    }

    /// A call muted for 40 s on a fake clock against the relay's idle check
    /// (steer-relay hub.ts: every min(idleTimeoutMs, 5 s), a pair without a
    /// frame for idleTimeoutMs gets `call_end` idle = `hungUp(idle)`): the
    /// muted capture (10 ms buffers) keeps one silent frame per `frameMs`
    /// going, so the call survives; a client that sent nothing is dropped by
    /// the same check.
    func testACallMutedFor40sOutlivesTheRelaysIdleCheck() {
        let idle = VoiceCall.Constants.idleTimeoutMs
        let perFrame = VoiceCall.Uplink.frameSamples
        let check = min(idle, 5_000)
        for sends in [true, false] {
            var state = [VoiceCall.Event.start(device: "mint"), .rang, .ready, .replyFinished, .mute].reduce(VoiceCall.idle, VoiceCall.callStep)
            var uplink = VoiceCall.Uplink()
            uplink.setMuted(state.muted)
            var lastFrameAt = 0
            var frames = 0
            for now in stride(from: 10, through: 40_000, by: 10) {
                if sends {
                    let out = uplink.push([Int16](repeating: 900, count: perFrame / 2))
                    XCTAssertTrue(out.allSatisfy { $0.allSatisfy { $0 == 0 } })
                    if !out.isEmpty {
                        frames += out.count
                        lastFrameAt = now
                    }
                }
                if now % check == 0, state.phase != .hidden, now - lastFrameAt >= idle {
                    state = VoiceCall.callStep(state, .hungUp(reason: "idle"))
                }
            }
            if sends {
                XCTAssertEqual(frames, 40_000 / VoiceCall.Constants.frameMs)
                XCTAssertEqual(state.phase, .listening)
                XCTAssertTrue(state.muted)
                XCTAssertNil(state.toast)
            } else {
                XCTAssertEqual(state.phase, .hidden)
                XCTAssertEqual(state.toast, "The call dropped")
            }
        }
    }

    func testHostEvents() {
        XCTAssertTrue(VoiceCall.Event(json: ["t": "rang"])?.isHostEvent == true)
        XCTAssertTrue(VoiceCall.Event(json: ["t": "ready"])?.isHostEvent == true)
        XCTAssertTrue(VoiceCall.Event(json: ["t": "noSound"])?.isHostEvent == true)
        XCTAssertNil(VoiceCall.Event(json: ["t": "connected"]))
        XCTAssertTrue(VoiceCall.Event(json: ["t": "speechStarted"])?.isHostEvent == true)
        XCTAssertTrue(VoiceCall.Event(json: ["t": "queued", "count": 2, "text": "x"])?.isHostEvent == true)
        XCTAssertFalse(VoiceCall.Event(json: ["t": "end"])?.isHostEvent == true)
        XCTAssertNil(VoiceCall.Event(json: ["t": "nope"]))
    }

    /// `callSpeech` = web `callSpeechOf` over the doctor's `speech` row.
    func testCallSpeechReadsTheDoctorRow() {
        func doctor(_ state: String, group: String = "voice") -> String {
            #"{"checkedAt":"2026-10-27T00:00:00Z","items":[{"key":"git","group":"required","state":"ok"},{"key":"speech","group":"\#(group)","state":"\#(state)"}]}"#
        }
        XCTAssertEqual(VoiceCall.callSpeech(ofJSON: doctor("ok")), .ok)
        XCTAssertEqual(VoiceCall.callSpeech(ofJSON: doctor("missing")), .noGpu)
        XCTAssertEqual(VoiceCall.callSpeech(ofJSON: doctor("downloading")), .preparing)
        XCTAssertEqual(VoiceCall.callSpeech(ofJSON: doctor("warn")), .preparing)
        XCTAssertNil(VoiceCall.callSpeech(ofJSON: doctor("ok", group: "agents")))
        XCTAssertNil(VoiceCall.callSpeech(ofJSON: #"{"items":[]}"#))
        XCTAssertNil(VoiceCall.callSpeech(ofJSON: nil))
    }

    func testTicketDecodes() throws {
        let data = Data(#"{"callId":"c1","ticket":"t1","url":"wss://relay/ws?ticket=t1","relayUrl":"wss://relay"}"#.utf8)
        XCTAssertEqual(try JSONDecoder().decode(VoiceCall.Ticket.self, from: data),
                       VoiceCall.Ticket(callId: "c1", ticket: "t1", url: "wss://relay/ws?ticket=t1"))
    }

    // #NEW-89: a late `wasSuspended` began never drops a running call.
    func testInterruptionIgnoresWasSuspended() {
        typealias Rules = VoiceCall.AudioSessionRules
        XCTAssertTrue(Rules.interruptionDropsCall(began: true, wasSuspended: false))
        XCTAssertFalse(Rules.interruptionDropsCall(began: true, wasSuspended: true))
        XCTAssertFalse(Rules.interruptionDropsCall(began: false, wasSuspended: false))
        XCTAssertFalse(Rules.interruptionDropsCall(began: false, wasSuspended: true))
    }

    // #NEW-89: AirPods mid-call (the built-in 48 kHz mic → a 16 kHz HFP
    // mic and back) rebuild the tap and resampler; no input drops the call.
    func testRouteChangeRebuildsTheCaptureChain() {
        typealias Rules = VoiceCall.AudioSessionRules
        let builtIn = Rules.InputFormat(sampleRate: 48_000, channels: 1)
        let airPods = Rules.InputFormat(sampleRate: 16_000, channels: 1)
        XCTAssertEqual(Rules.captureChange(built: builtIn, route: builtIn), .keep)
        XCTAssertEqual(Rules.captureChange(built: builtIn, route: airPods), .rebuild)
        XCTAssertEqual(Rules.captureChange(built: airPods, route: builtIn), .rebuild)
        XCTAssertEqual(Rules.captureChange(built: builtIn, route: .init(sampleRate: 48_000, channels: 2)), .rebuild)
        XCTAssertEqual(Rules.captureChange(built: nil, route: builtIn), .rebuild)
        XCTAssertEqual(Rules.captureChange(built: builtIn, route: .init(sampleRate: 0, channels: 0)), .drop)
        XCTAssertEqual(Rules.captureChange(built: builtIn, route: .init(sampleRate: 16_000, channels: 0)), .drop)
        XCTAssertEqual(Rules.tapFrames(sampleRate: 48_000), 960)
        XCTAssertEqual(Rules.tapFrames(sampleRate: 16_000), 320)
    }
}
