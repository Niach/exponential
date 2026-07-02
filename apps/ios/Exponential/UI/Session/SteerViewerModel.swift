import ExpCore
import Foundation

// Viewer-side steer relay client (masterplan §5c) — the iOS mirror of the web
// `SteerViewer` in apps/web/src/components/steer-terminal.tsx. Mints a viewer
// ticket over tRPC, dials the returned ws(s) URL with URLSessionWebSocketTask,
// sends `join`, then folds the room's frames into a `VTScreen`:
//   binary frames: opcode 0x01 + verbatim PTY bytes → VTScreen.feed
//   text frames:   presence → viewer bar, resize → grid reflow, bye → ended
// While holding the steer claim, the input bar's keystrokes flow back as
// `input` frames. The relay enforces perm server-side; the ticket's claims
// only decide which controls we bother to render.

struct SteerPresenceViewer: Identifiable, Equatable {
    let userId: String
    let name: String
    let perm: String
    var id: String { userId }
}

@MainActor @Observable
final class SteerViewerModel {
    enum Phase: Equatable {
        case idle
        case connecting
        case live
        /// Session over (relay `bye` / room gone). Terminal keeps the last frame.
        case ended(detail: String?)
        /// Unexpected socket loss / ticket failure — reconnect offered.
        case closed(detail: String?)
    }

    private(set) var phase: Phase = .idle
    private(set) var screen = VTScreen()
    private(set) var viewers: [SteerPresenceViewer] = []
    private(set) var steererId: String?
    /// The minted ticket's perm claim (`view` or `steer`), display-gating only.
    private(set) var perm: String = "view"

    var isSteering: Bool { steererId != nil && steererId == currentUserId }
    var canSteer: Bool { perm == "steer" }
    var remoteSteererName: String? {
        guard let steererId, steererId != currentUserId else { return nil }
        return viewers.first { $0.userId == steererId }?.name ?? "Someone"
    }

    private let accountId: String
    private let codingSessionId: String
    private let currentUserId: String?
    private let steerApi: SteerApi

    private var task: URLSessionWebSocketTask?
    private var connected = false
    private var stopped = false
    private var sawEnd = false
    private var endDetail: String?

    // Relay rejects input frames > 8 KiB; chunk pastes well under that.
    private static let inputChunkChars = 4096

    init(accountId: String, codingSessionId: String, currentUserId: String?, steerApi: SteerApi) {
        self.accountId = accountId
        self.codingSessionId = codingSessionId
        self.currentUserId = currentUserId
        self.steerApi = steerApi
    }

    func connect() {
        guard phase != .connecting, phase != .live else { return }
        stopped = false
        sawEnd = false
        endDetail = nil
        phase = .connecting
        viewers = []
        steererId = nil
        screen = VTScreen()
        Task { await dial() }
    }

    func disconnect() {
        stopped = true
        connected = false
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        viewers = []
        steererId = nil
        if phase == .connecting || phase == .live { phase = .idle }
    }

    // MARK: - Steering

    func claim() { sendText(#"{"t":"claim"}"#) }
    func release() { sendText(#"{"t":"release"}"#) }

    /// Send raw keystroke text (already includes any control bytes).
    func sendInput(_ text: String) {
        guard isSteering, !text.isEmpty else { return }
        var rest = Substring(text)
        while !rest.isEmpty {
            let chunk = String(rest.prefix(Self.inputChunkChars))
            rest = rest.dropFirst(chunk.count)
            let frame: [String: Any] = ["t": "input", "data": chunk]
            if let data = try? JSONSerialization.data(withJSONObject: frame),
               let json = String(data: data, encoding: .utf8) {
                sendText(json)
            }
        }
    }

    // MARK: - Connect lifecycle

    private func dial() async {
        let ticket: SteerTicket
        do {
            ticket = try await steerApi.mintViewerTicket(accountId: accountId, codingSessionId: codingSessionId)
        } catch {
            phase = .closed(detail: "Couldn't get a viewer ticket. \(error.localizedDescription)")
            return
        }
        guard !stopped else { return }
        guard !ticket.isDisabled, let url = ticket.connectURL() else {
            phase = .closed(detail: "Live steering is unavailable on this instance.")
            return
        }
        perm = Self.decodeTicketPerm(ticket.ticket ?? "")

        let t = URLSession.shared.webSocketTask(with: url)
        task = t
        connected = true
        t.resume()
        sendText(#"{"t":"join"}"#)
        phase = .live
        receiveLoop()
    }

    private func receiveLoop() {
        // The completion runs off the main actor; extract Sendable payloads and
        // hop back (same shape as MacSteerPublisher's loop).
        task?.receive { [weak self] result in
            switch result {
            case .success(.string(let text)):
                Task { @MainActor in
                    self?.onText(text)
                    self?.rearm()
                }
            case .success(.data(let data)):
                Task { @MainActor in
                    self?.onBinary(data)
                    self?.rearm()
                }
            case .success:
                Task { @MainActor in self?.rearm() }
            case .failure:
                Task { @MainActor in self?.onSocketClosed() }
            }
        }
    }

    private func rearm() {
        if !stopped { receiveLoop() }
    }

    private func onBinary(_ data: Data) {
        guard data.count >= 1, data.first == 0x01 else { return }
        screen.feed(data.dropFirst())
    }

    private func onText(_ text: String) {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let t = obj["t"] as? String else { return }
        switch t {
        case "presence":
            let raw = obj["viewers"] as? [[String: Any]] ?? []
            viewers = raw.compactMap { v in
                guard let userId = v["userId"] as? String else { return nil }
                return SteerPresenceViewer(
                    userId: userId,
                    name: (v["name"] as? String) ?? userId,
                    perm: (v["perm"] as? String) ?? "view"
                )
            }
            steererId = obj["steererId"] as? String
        case "resize":
            if let cols = obj["cols"] as? Int, let rows = obj["rows"] as? Int {
                screen.resize(cols: cols, rows: rows)
            }
        case "bye":
            sawEnd = true
            let outcome = obj["outcome"] as? String
            endDetail = (outcome != nil && outcome != "ended") ? outcome : nil
        case "error":
            let code = (obj["code"] as? String) ?? "error"
            if code == "no_such_session" {
                sawEnd = true
                endDetail = "The terminal isn't live on the relay yet — the desktop may still be connecting."
            } else {
                endDetail = (obj["message"] as? String) ?? code
            }
        default:
            break
        }
    }

    private func onSocketClosed() {
        guard !stopped else { return }
        connected = false
        task = nil
        viewers = []
        steererId = nil
        phase = sawEnd
            ? .ended(detail: endDetail)
            : .closed(detail: endDetail ?? "Connection lost.")
    }

    private func sendText(_ text: String) {
        guard connected, let task else { return }
        task.send(.string(text)) { _ in }
    }

    /// The ticket is `base64url(JSON claims).base64url(sig)` — decode the perm
    /// claim for display gating (the relay enforces it server-side regardless).
    static func decodeTicketPerm(_ ticket: String) -> String {
        guard let dot = ticket.firstIndex(of: ".") else { return "view" }
        var b64 = String(ticket[..<dot])
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while b64.count % 4 != 0 { b64 += "=" }
        guard let data = Data(base64Encoded: b64),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let perm = obj["perm"] as? String else { return "view" }
        return perm == "steer" ? "steer" : "view"
    }
}
