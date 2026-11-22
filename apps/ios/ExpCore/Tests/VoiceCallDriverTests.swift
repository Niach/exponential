import Foundation
import XCTest
@testable import ExpCore

// #EXP-1993: every `caller.cases` row of fixtures/voice-call.json through
// the REAL `VoiceCallDriver` (what `VoiceCallClient` runs) with a fake
// socket, audio engine, seal line and platform: each fixture event becomes
// what the app would see (the tap, the hold's answer, the permission and
// `startCall`, a frame off the socket, the engine's or the line's failure,
// the ring's timer), and the calls the driver makes for that event are
// asserted against the effects the row expects.
@MainActor
final class VoiceCallDriverTests: XCTestCase {
    private final class Log: @unchecked Sendable {
        var calls: [String] = []
        func add(_ call: String) { calls.append(call) }
    }

    private final class FakeTimer: CallTimer {
        let label: String
        let body: @MainActor () -> Void
        let log: Log
        var live = true
        init(_ label: String, _ body: @escaping @MainActor () -> Void, _ log: Log) {
            self.label = label
            self.body = body
            self.log = log
        }
        func cancel() {
            guard live else { return }
            live = false
            log.add("cancel \(label)")
        }
    }

    private final class FakeSocket: CallSocket, @unchecked Sendable {
        let log: Log
        var inbound: CallInbound?
        init(_ log: Log) { self.log = log }
        func send(text: String) { log.add("send \(text)") }
        func sendLast(text: String) { log.add("sendLast \(text)") }
        func send(binary: Data) { log.add("send binary") }
        func send(audio: Data) { log.add("send audio") }
        func resume(_ inbound: CallInbound) {
            self.inbound = inbound
            log.add("resume")
        }
        func close() { log.add("socket.close") }
    }

    private final class FakeAudio: CallAudio, @unchecked Sendable {
        let log: Log
        let onFailure: @Sendable (String) -> Void
        init(_ log: Log, _ onFailure: @escaping @Sendable (String) -> Void) {
            self.log = log
            self.onFailure = onFailure
        }
        func open() async throws { log.add("audio.open") }
        func stop() { log.add("audio.stop") }
        func setMuted(_ muted: Bool) { log.add("audio.mute \(muted)") }
        func flush() { log.add("audio.flush") }
        func receive(_ frame: Data) { log.add("audio.receive") }
    }

    private final class FakeSeal: CallSeal, @unchecked Sendable {
        let log: Log
        init(_ log: Log) { self.log = log }
        func inbound(text: String) -> String? { text }
        func inbound(frame: Data) -> Data? { frame }
        func uplink(_ clear: Data) -> Data? { clear }
        var muted = false {
            didSet { log.add("seal.muted \(muted)") }
        }
        func close() { log.add("seal.close") }
    }

    private final class FakeWatch: CallPathWatch, @unchecked Sendable {
        let log: Log
        init(_ log: Log) { self.log = log }
        func heard() {}
        func beat() { log.add("beat") }
        func dead() -> Bool { false }
    }

    private struct FakeKeys: CallKeySource {
        func ready() async -> Bool { true }
        func target(agentId: String) async -> ServerSendTarget? { nil }
        func secret(rootId: String, epoch: UInt32) async -> Data? { nil }
        func highestEpoch(rootId: String) -> UInt32? { nil }
    }

    private struct Refusal: Error { let message: String }

    @MainActor
    private final class FakePlatform: CallPlatform {
        let log: Log
        var answerHold: (@MainActor (Bool, String?) -> Void)?
        var microphone: CheckedContinuation<Bool, Never>?
        /// What the next `startCall` answers.
        var answer: Result<VoiceCall.Ticket, Error>?
        var timers: [FakeTimer] = []
        var socket: FakeSocket?
        var audio: FakeAudio?
        var sealIo: CallSealLine.Io?

        init(_ log: Log) { self.log = log }

        func hold(speaker: Bool, answer: @escaping @MainActor (Bool, String?) -> Void) {
            log.add("hold speaker \(speaker)")
            answerHold = answer
        }
        func ringStart() { log.add("ringStart") }
        func ringStop() { log.add("ringStop") }
        func setSpeaker(_ on: Bool) { log.add("speaker \(on)") }
        func toast(_ text: String) { log.add("toast \(text)") }
        func leaveAudioSession() { log.add("leaveAudioSession") }
        func requestMicrophone() async -> Bool {
            log.add("requestMicrophone")
            return await withCheckedContinuation { microphone = $0 }
        }
        func startCall(agentId: String, seal: Bool) async throws -> VoiceCall.Ticket {
            log.add("startCall \(agentId) seal \(seal)")
            return try (answer ?? .failure(Refusal(message: "no answer"))).get()
        }
        func refusal(_ error: Error) -> String { (error as? Refusal)?.message ?? "?" }
        func openSocket(_ url: URL) -> CallSocket {
            log.add("openSocket \(url.absoluteString)")
            let socket = FakeSocket(log)
            self.socket = socket
            return socket
        }
        func makeSeal(_ sealed: VoiceCallDriver.Sealed, io: CallSealLine.Io) -> CallSeal? {
            log.add("makeSeal \(sealed.deviceId)")
            sealIo = io
            return FakeSeal(log)
        }
        func makeAudio(
            onUplink: @escaping @Sendable (Data) -> Void,
            onLevel: @escaping @Sendable (Double, Bool) -> Void,
            onFailure: @escaping @Sendable (String) -> Void
        ) -> CallAudio {
            log.add("makeAudio")
            let audio = FakeAudio(log, onFailure)
            self.audio = audio
            return audio
        }
        func watchPath() -> CallPathWatch {
            log.add("watchPath")
            return FakeWatch(log)
        }
        func after(ms: Int, _ body: @escaping @MainActor () -> Void) -> CallTimer {
            log.add("after \(ms)")
            let timer = FakeTimer("after \(ms)", body, log)
            timers.append(timer)
            return timer
        }
        func every(ms: Int, _ body: @escaping @MainActor () -> Void) -> CallTimer {
            log.add("every \(ms)")
            let timer = FakeTimer("every \(ms)", body, log)
            timers.append(timer)
            return timer
        }
        func keepAwake(_ awake: Bool) {}

        func grant(_ granted: Bool) {
            let microphone = microphone
            self.microphone = nil
            microphone?.resume(returning: granted)
        }
    }

    // MARK: - The replay

    private func cases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/etalk/fixtures/voice-call.json")
        let fixture = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        return try XCTUnwrap((fixture["caller"] as? [String: Any])?["cases"] as? [[String: Any]])
    }

    private static let relay = "wss://relay.test/call?ticket=t"

    private func json(_ object: Any) throws -> String {
        String(decoding: try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]), as: UTF8.self)
    }

    /// What the app's ports see for one fixture event.
    private func stimulate(_ event: [String: Any], _ driver: VoiceCallDriver, _ platform: FakePlatform) throws {
        let stage = driver.caller.stage
        switch event["t"] as? String {
        case "start":
            driver.start(
                agentId: "agent-1", keys: FakeKeys(), device: try XCTUnwrap(event["device"] as? String),
                language: event["language"] as? String, fallback: event["fallback"] as? Bool ?? false
            )
        case "held":
            platform.answerHold?(try XCTUnwrap(event["ok"] as? Bool), event["code"] as? String)
        case "ticket":
            let sealed = event["sealed"] as? Bool ?? false
            platform.answer = .success(VoiceCall.Ticket(
                callId: "call-1", ticket: "t", url: Self.relay, deviceId: sealed ? "d-mint" : nil,
                device: event["device"] as? String, fallback: event["fallback"] as? Bool, seal: sealed
            ))
            platform.grant(true)
        case "refused":
            platform.answer = .failure(Refusal(message: try XCTUnwrap(event["message"] as? String)))
            platform.grant(true)
        case "frame":
            platform.socket?.inbound?.text(try json(try XCTUnwrap(event["frame"])))
        case "closed":
            platform.socket?.inbound?.closed()
        case "failed":
            let code = try XCTUnwrap(event["code"] as? String)
            if event["told"] as? Bool == true {
                platform.sealIo?.fail(code)
            } else if stage == .asking {
                // iOS: the microphone refused once the ring started.
                XCTAssertEqual(code, "not_granted")
                platform.grant(false)
            } else {
                platform.audio?.onFailure(code)
            }
        case "ringTimeout":
            platform.timers.first { $0.label.hasPrefix("after") && $0.live }?.body()
        case "mute": driver.toggleMute()
        case "speaker": driver.toggleSpeaker()
        case "end": driver.end()
        case let t: XCTFail("unknown input \(String(describing: t))")
        }
    }

    /// The calls the driver makes for one row's effects, given what the
    /// call holds before them.
    private struct Holds {
        var ringTimer = false, watch = false, audio = false, seal = false, socket = false
    }

    private func calls(_ event: [String: Any], _ effects: [[String: Any]], stage: VoiceCall.Caller.Stage, holds: inout Holds) throws -> [String] {
        var calls: [String] = []
        let t = event["t"] as? String
        if (t == "ticket" || t == "refused"), stage == .asking { calls.append("startCall agent-1 seal true") }
        for effect in effects {
            switch effect["do"] as? String {
            case "hold": calls.append("hold speaker true")
            case "ring":
                let ms = try XCTUnwrap(effect["ms"] as? Int)
                calls += ["ringStart", "after \(ms)"]
                holds.ringTimer = true
            case "ticket": calls.append("requestMicrophone")
            case "dial":
                let sealed = event["sealed"] as? Bool ?? false
                calls.append("openSocket \(Self.relay)")
                if sealed { calls.append("makeSeal d-mint") }
                calls += ["watchPath", "every 1000", "makeAudio", "resume"]
                holds.watch = true
                holds.audio = true
                holds.seal = sealed
                holds.socket = true
            case "startAudio": calls.append("audio.open")
            case "stopRing":
                if holds.ringTimer { calls.append("cancel after \(VoiceCall.Constants.maxRingMs)") }
                holds.ringTimer = false
                calls.append("ringStop")
            case "flush": calls.append("audio.flush")
            case "mute":
                let muted = try XCTUnwrap(effect["muted"] as? Bool)
                calls.append("audio.mute \(muted)")
                if holds.seal { calls.append("seal.muted \(muted)") }
            case "speaker": calls.append("speaker \(try XCTUnwrap(effect["on"] as? Bool))")
            case "send":
                let frame = try XCTUnwrap(effect["frame"] as? [String: Any])
                guard holds.socket else { break }
                if frame["t"] as? String == "call_end" {
                    calls.append("sendLast \(try json(frame))")
                    holds.socket = false
                } else {
                    calls.append("send \(try json(frame))")
                }
            case "alive": calls.append("beat")
            case "release":
                if holds.ringTimer { calls.append("cancel after \(VoiceCall.Constants.maxRingMs)") }
                if holds.watch { calls.append("cancel every 1000") }
                calls += ["ringStop", holds.audio ? "audio.stop" : "leaveAudioSession"]
                if holds.seal { calls.append("seal.close") }
                if holds.socket { calls.append("socket.close") }
                holds = Holds()
            case "toast": calls.append("toast \(try XCTUnwrap(effect["text"] as? String))")
            case let other: XCTFail("unknown effect \(String(describing: other))")
            }
        }
        return calls
    }

    /// Lets the driver's tasks and hops run until `count` calls are in (and
    /// a little longer, so a call too many shows).
    private func settle(_ log: Log, count: Int) async throws {
        var spins = 0
        while log.calls.count < count, spins < 500 {
            try await Task.sleep(nanoseconds: 1_000_000)
            spins += 1
        }
        for _ in 0..<5 { try await Task.sleep(nanoseconds: 1_000_000) }
    }

    func testEveryCaseRunsThroughTheDriver() async throws {
        let cases = try cases()
        XCTAssertGreaterThanOrEqual(cases.count, 31)
        for entry in cases {
            let name = try XCTUnwrap(entry["name"] as? String)
            let events = try XCTUnwrap(entry["events"] as? [[String: Any]], name)
            let effects = try XCTUnwrap(entry["effects"] as? [[[String: Any]]], name)
            let log = Log()
            let platform = FakePlatform(log)
            let driver = VoiceCallDriver(platform: platform)
            var holds = Holds()
            for (index, event) in events.enumerated() {
                log.calls.removeAll()
                let expected = try calls(event, effects[index], stage: driver.caller.stage, holds: &holds)
                try stimulate(event, driver, platform)
                try await settle(log, count: expected.count)
                XCTAssertEqual(log.calls, expected, "\(name): event \(index) \(event)")
            }
            XCTAssertEqual(driver.caller.stage.rawValue, entry["stage"] as? String, name)
            XCTAssertEqual(driver.state.phase.rawValue, entry["phase"] as? String, name)
            if let device = entry["device"] as? String { XCTAssertEqual(driver.state.device, device, name) }
            if let fallback = entry["fallback"] as? Bool { XCTAssertEqual(driver.state.fallback, fallback, name) }
            if let muted = entry["muted"] as? Bool { XCTAssertEqual(driver.state.muted, muted, name) }
            if let speaker = entry["speaker"] as? Bool { XCTAssertEqual(driver.state.speaker, speaker, name) }
            if driver.caller.stage == .idle {
                // Nothing of an ended call is left to fire.
                XCTAssertEqual(platform.timers.filter(\.live).map(\.label), [], name)
            }
            // A permission still asked is answered, so no task waits for ever.
            platform.grant(false)
        }
    }

    /// The timer's zero: answered at the pickup, gone with the call.
    func testTheTimerStartsAtThePickup() async throws {
        let log = Log()
        let platform = FakePlatform(log)
        let driver = VoiceCallDriver(platform: platform)
        driver.start(agentId: "agent-1", keys: nil, device: "mint", language: "en", fallback: false)
        platform.answerHold?(true, nil)
        platform.answer = .success(VoiceCall.Ticket(callId: "call-1", ticket: "t", url: Self.relay))
        try await settle(log, count: 4)
        platform.grant(true)
        try await settle(log, count: 10)
        XCTAssertTrue(log.calls.contains("startCall agent-1 seal false"), "no key holder: no claim")
        for frame in [#"{"t":"call_ready"}"#, #"{"t":"call_event","event":{"t":"rang"}}"#] {
            platform.socket?.inbound?.text(frame)
        }
        try await settle(log, count: 11)
        XCTAssertNil(driver.answeredAt, "still ringing")
        platform.socket?.inbound?.text(#"{"t":"call_event","event":{"t":"ready"}}"#)
        try await settle(log, count: 13)
        XCTAssertNotNil(driver.answeredAt)
        driver.end()
        XCTAssertNil(driver.answeredAt)
        XCTAssertEqual(driver.state.phase, .hidden)
    }
}
