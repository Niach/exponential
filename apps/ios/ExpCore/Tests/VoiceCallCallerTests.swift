import Foundation
import XCTest
@testable import ExpCore

// #EXP-1993: the phone caller's session, every `caller.cases` row of
// fixtures/voice-call.json (Android replays the same rows in
// `VoiceCallCallerTest`): per event the effects in order, then the final
// stage and phase.
final class VoiceCallCallerTests: XCTestCase {
    private typealias Caller = VoiceCall.Caller

    private struct Unknown: Error { let t: String? }

    private func cases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/etalk/fixtures/voice-call.json")
        let fixture = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let caller = try XCTUnwrap(fixture["caller"] as? [String: Any])
        let cases = try XCTUnwrap(caller["cases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 31)
        return cases
    }

    private func input(_ e: [String: Any]) throws -> Caller.Input {
        switch e["t"] as? String {
        case "start":
            return .start(device: try XCTUnwrap(e["device"] as? String), language: e["language"] as? String, fallback: e["fallback"] as? Bool ?? false)
        case "held": return .held(ok: try XCTUnwrap(e["ok"] as? Bool), code: e["code"] as? String)
        case "ticket": return .ticket(device: e["device"] as? String, fallback: e["fallback"] as? Bool, sealed: e["sealed"] as? Bool ?? false)
        case "refused": return .refused(message: try XCTUnwrap(e["message"] as? String))
        case "frame": return .frame(Caller.Frame(json: try XCTUnwrap(e["frame"] as? [String: Any])))
        case "closed": return .closed
        case "failed": return .failed(code: try XCTUnwrap(e["code"] as? String), told: e["told"] as? Bool ?? false)
        case "ringTimeout": return .ringTimeout
        case "mute": return .mute
        case "speaker": return .speaker
        case "end": return .end
        case let t: throw Unknown(t: t)
        }
    }

    private func json(_ effect: Caller.Effect) -> NSDictionary {
        let dictionary: [String: Any] = switch effect {
        case .hold: ["do": "hold"]
        case let .ring(ms): ["do": "ring", "ms": ms]
        case .ticket: ["do": "ticket"]
        case .dial: ["do": "dial"]
        case .startAudio: ["do": "startAudio"]
        case .stopRing: ["do": "stopRing"]
        case .flush: ["do": "flush"]
        case let .mute(muted): ["do": "mute", "muted": muted]
        case let .speaker(on): ["do": "speaker", "on": on]
        case let .send(frame): ["do": "send", "frame": frame.json]
        case .alive: ["do": "alive"]
        case .release: ["do": "release"]
        case let .toast(text): ["do": "toast", "text": text]
        }
        return NSDictionary(dictionary: dictionary)
    }

    func testEveryCaseReplays() throws {
        for entry in try cases() {
            let name = try XCTUnwrap(entry["name"] as? String)
            let events = try XCTUnwrap(entry["events"] as? [[String: Any]], name)
            let effects = try XCTUnwrap(entry["effects"] as? [[[String: Any]]], name)
            XCTAssertEqual(events.count, effects.count, name)
            var state = Caller.State.idle
            for (index, event) in events.enumerated() {
                let stepped = Caller.step(state, try input(event))
                state = stepped.state
                XCTAssertEqual(stepped.effects.map(json), effects[index].map { NSDictionary(dictionary: $0) }, "\(name): event \(index) \(event)")
            }
            XCTAssertEqual(state.stage.rawValue, entry["stage"] as? String, name)
            XCTAssertEqual(state.call.phase.rawValue, entry["phase"] as? String, name)
            if let device = entry["device"] as? String { XCTAssertEqual(state.call.device, device, name) }
            if let fallback = entry["fallback"] as? Bool { XCTAssertEqual(state.call.fallback, fallback, name) }
            if let muted = entry["muted"] as? Bool { XCTAssertEqual(state.call.muted, muted, name) }
            if let speaker = entry["speaker"] as? Bool { XCTAssertEqual(state.call.speaker, speaker, name) }
        }
    }

    /// What goes on the socket is the fixture's frame as JSON text.
    func testTheSentFramesAreTheirJson() throws {
        for frame: Caller.Outgoing in [.end(reason: nil), .end(reason: "dropped"), .mute(true), .mute(false)] {
            let parsed = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(frame.text.utf8)) as? [String: Any])
            XCTAssertEqual(NSDictionary(dictionary: parsed), NSDictionary(dictionary: frame.json))
        }
        XCTAssertEqual(Caller.Frame(text: "not json"), nil)
        XCTAssertEqual(Caller.Frame(text: #"{"t":"call_event","event":{"t":"rang"}}"#), .event(.rang))
    }
}
