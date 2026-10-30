// #EXP-1441: the Swift binding suite. Replays packages/etalk/fixtures/voice-call.json
// through the facade (the events built as Swift enums, so every case crosses
// the binding both ways) and round-trips a call's audio through the codec.
//   bash apps/desktop/crates/etalk-ffi/run-binding-tests.sh swift
import Foundation

var failures = 0
var checks = 0

func check(_ ok: Bool, _ message: @autoclosure () -> String) {
    checks += 1
    if !ok {
        failures += 1
        print("FAIL: \(message())")
    }
}

let path = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : nil
let text = path.flatMap { try? String(contentsOfFile: $0, encoding: .utf8) } ?? callSpecJson()
check(path == nil || text == callSpecJson(), "the facade embeds the committed fixture")
let fixture = try! JSONSerialization.jsonObject(with: Data(text.utf8)) as! [String: Any]

func str(_ v: Any?) -> String? { v as? String }
func num(_ v: Any?) -> NSNumber? { v as? NSNumber }

func platform(_ v: Any?) -> TalkCallPlatform {
    switch str(v) {
    case "desktop": return .desktop
    case "phone": return .phone
    default: return .web
    }
}

func speech(_ v: String) -> TalkCallSpeech {
    switch v {
    case "ok": return .ok
    case "noGpu": return .noGpu
    case "failed": return .failed
    default: return .preparing
    }
}

func button(_ v: Any?) -> TalkCallButton {
    switch str(v) {
    case "mute": return .mute
    case "speaker": return .speaker
    default: return .end
    }
}

func event(_ e: [String: Any]) -> TalkCallEvent {
    let text = str(e["text"]) ?? ""
    switch str(e["t"])! {
    case "start": return .start(device: str(e["device"])!, language: str(e["language"]))
    case "rang": return .rang
    case "ready": return .ready
    case "noSound": return .noSound
    case "partial": return .partial(text: text)
    case "said": return .said(text: text)
    case "speechStarted": return .speechStarted
    case "queued": return .queued(count: num(e["count"])!.uint32Value, text: text)
    case "replyStarted": return .replyStarted(text: text)
    case "replyFinished": return .replyFinished
    case "mute": return .mute
    case "unmute": return .unmute
    case "speaker": return .speaker(on: e["on"] as! Bool)
    case "end": return .end
    case "hungUp": return .hungUp(reason: str(e["reason"]))
    case "failed": return .failed(code: str(e["code"])!)
    case let t: fatalError("unknown event \(t)")
    }
}

// availability + speech
for c in fixture["availabilityCases"] as! [[String: Any]] {
    let i = c["input"] as! [String: Any]
    let e = c["expected"] as! [String: Any]
    let reason = callUnavailable(input: TalkCallAvailabilityInput(
        agent: i["agent"] as! Bool, platform: platform(i["platform"]), device: str(i["device"]),
        online: i["online"] as! Bool, here: i["here"] as! Bool, speech: str(i["speech"]).map(speech),
        busy: i["busy"] as! Bool, meeting: (i["meeting"] as? Bool) ?? false))
    check(reason == str(e["reason"]), "availability \(c["name"]!): \(String(describing: reason))")
    check((reason == nil) == (e["enabled"] as! Bool), "availability \(c["name"]!) enabled")
}
for c in fixture["speechCases"] as! [[String: Any]] {
    check(callSpeechOf(state: str(c["state"])) == str(c["expected"]).map(speech), "speech \(c["name"]!)")
}

// the flow
let flows = fixture["flowCases"] as! [[String: Any]]
check(flows.count >= 41, "41+ flow cases")
for c in flows {
    let name = str(c["name"])!
    var state = callIdle()
    for e in c["events"] as! [[String: Any]] {
        let native = event(e)
        // The wire form of the same event decodes to the same enum.
        let wire = String(data: try! JSONSerialization.data(withJSONObject: e), encoding: .utf8)!
        check(callEventOfWire(json: wire) == native, "\(name): wire \(wire)")
        state = callStep(state: state, event: native)
    }
    let view = callView(state: state, platform: platform(c["platform"]))
    if let expected = c["view"] as? [String: Any] {
        guard let view else { check(false, "\(name): hidden"); continue }
        check(view.activity == str(expected["activity"]), "\(name): activity \(view.activity)")
        check(view.status == str(expected["status"]), "\(name): status \(String(describing: view.status))")
        check(view.caption == str(expected["caption"]), "\(name): caption \(view.caption)")
        check(view.muted == expected["muted"] as! Bool, "\(name): muted")
        check(view.speaker == expected["speaker"] as! Bool, "\(name): speaker")
        check(view.buttons == (expected["buttons"] as! [Any]).map(button), "\(name): buttons \(view.buttons)")
        check(view.end == str(expected["end"]), "\(name): end \(view.end)")
    } else {
        check(view == nil, "\(name): expected hidden")
    }
    check(state.toast == str(c["toast"]), "\(name): toast \(String(describing: state.toast))")
}

// the ring + the timer
for c in fixture["ringCases"] as! [[String: Any]] {
    check(callRingMs(random: num(c["random"])!.doubleValue) == num(c["ringMs"])!.uint64Value, "ring \(c)")
}
for c in fixture["answerCases"] as! [[String: Any]] {
    let answered = callAnswerMs(random: num(c["random"])!.doubleValue, readyMs: num(c["readyMs"])?.uint64Value)
    check(answered == num(c["answeredMs"])?.uint64Value, "answer \(c["name"]!)")
}
for c in fixture["ringToneCases"] as! [[String: Any]] {
    check(callRingToneOn(ms: num(c["ms"])!.int64Value) == c["on"] as! Bool, "ring tone \(c)")
}
for c in fixture["timerCases"] as! [[String: Any]] {
    check(callTimer(ms: num(c["ms"])!.uint64Value) == str(c["text"]), "timer \(c)")
}
for _ in 0..<50 { let r = callRingRandom(); check(r >= 0 && r < 1, "random \(r)") }
let k = callConstants()
check(k.minRingMs == num((fixture["constants"] as! [String: Any])["minRingMs"])!.uint64Value, "constants")
check(callGreeting(language: "de") == "Hallo?" && callGreeting(language: nil) == "Hello?", "greeting")
check(callWireEvent(event: .mute) == nil, "mute stays local")

// the codec: a 440 Hz tone, one packet lost and recovered by the uplink's FEC
do {
    let encoder = try TalkEncoder(rate: k.upRate, bitrate: Int32(k.upKbps * 1000), lossPercent: 10)
    let frame = Int(encoder.frameSamples())
    check(frame == Int(k.upRate * k.frameMs / 1000), "frame samples")
    let decoder = try TalkDecoder(rate: k.upRate)
    let uplink = try TalkUplink(rate: k.upRate)
    var energy = 0.0
    for n in 0..<25 {
        let pcm = (0..<frame).map { i in Int16(sin(Double(n * frame + i) * 440 * 2 * .pi / Double(k.upRate)) * 8000) }
        let packet = try encoder.encode(pcm: pcm)
        check(!packet.isEmpty && packet.count <= Int(k.maxFrameBytes), "packet \(n) size \(packet.count)")
        let out = try decoder.decode(packet: packet)
        check(out.count == frame, "decoded \(out.count)")
        energy += out.reduce(0.0) { $0 + Double($1) * Double($1) }
        if n != 10 {
            let pushed = uplink.push(seq: UInt16(n), packet: packet)
            check(pushed.count == (n == 11 ? 2 * frame : frame), "uplink \(n): \(pushed.count)")
        }
    }
    check(energy > 0, "the tone decodes to sound")
    check(try decoder.decode(packet: Data()).count == frame, "an empty packet conceals")
    do { _ = try encoder.encode(pcm: [0, 0, 0]); check(false, "a short frame throws") } catch TalkError.Invalid { } catch { check(false, "\(error)") }
    do { _ = try TalkEncoder(rate: 44100, bitrate: 24000, lossPercent: nil); check(false, "44.1 kHz throws") } catch TalkError.Invalid { } catch { check(false, "\(error)") }
    check(codecBackend() == "opus-rs", "backend")
} catch {
    check(false, "codec: \(error)")
}

print("\(checks) checks, \(failures) failures")
exit(failures == 0 ? 0 : 1)
