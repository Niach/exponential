import Foundation

// The steer relay wire protocol — the Swift mirror of apps/steer-relay/src/
// protocol.ts (masterplan §3.2). Control frames are JSON `{t, …}`; terminal
// output is a raw binary frame (opcode byte 0x01 + verbatim PTY bytes).

/// Opcode byte prefixing a binary terminal-output frame (OUTPUT_OPCODE).
let steerOutputOpcode: UInt8 = 0x01

/// Inbound control frames the desktop handles (relay → desktop).
enum SteerInbound {
    /// Control socket: remote "Start on my desktop".
    case startSession(issueId: String)
    /// Publisher socket: keystrokes from the steering viewer (utf8).
    case input(data: String)
    /// Publisher socket: a viewer joined / needs a repaint → replay recent output.
    case resync
    /// Publisher socket: kill-switch → tear the session down.
    case kill
    /// Presence update (drives a "remote steering — <name>" banner later).
    case presence(steererId: String?)
    /// Anything else (ignored).
    case unknown(type: String)

    /// Decode a JSON text control frame. Returns nil for non-JSON / missing `t`.
    static func decode(_ text: String) -> SteerInbound? {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let t = obj["t"] as? String else { return nil }
        switch t {
        case "start_session":
            return .startSession(issueId: (obj["issueId"] as? String) ?? "")
        case "input":
            return .input(data: (obj["data"] as? String) ?? "")
        case "resync":
            return .resync
        case "kill":
            return .kill
        case "presence":
            return .presence(steererId: obj["steererId"] as? String)
        default:
            return .unknown(type: t)
        }
    }
}

/// Outbound frame builders (desktop → relay).
enum SteerOutbound {
    /// Control socket presence announce.
    static func online(deviceId: String, deviceLabel: String) -> String {
        json(["t": "online", "deviceId": deviceId, "deviceLabel": deviceLabel])
    }

    /// Publisher socket registration for a session's room.
    static func hello(sessionId: String, issueId: String, cols: Int, rows: Int) -> String {
        json(["t": "hello", "sessionId": sessionId, "issueId": issueId, "cols": cols, "rows": rows])
    }

    /// Publisher: local terminal geometry changed (viewers reflow).
    static func resize(cols: Int, rows: Int) -> String {
        json(["t": "resize", "cols": cols, "rows": rows])
    }

    /// Publisher: session ended.
    static func bye(outcome: String) -> String {
        json(["t": "bye", "outcome": outcome])
    }

    /// A binary terminal-output frame: opcode 0x01 + verbatim PTY bytes.
    static func outputFrame(_ bytes: Data) -> Data {
        var frame = Data([steerOutputOpcode])
        frame.append(bytes)
        return frame
    }

    private static func json(_ obj: [String: Any]) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: obj),
              let s = String(data: data, encoding: .utf8) else { return "{}" }
        return s
    }
}
