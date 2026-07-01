import ExpCore
import Foundation

/// One steer publisher per running coding session (keyed by `coding_sessions.id`,
/// masterplan §3.3). It dials the relay outbound with a short-lived publisher
/// ticket, registers the session's room (`hello`), TEEs the session's PTY output
/// to the relay as binary `0x01` frames, and injects a remote steerer's keystrokes
/// back into the SAME PTY the local keyboard writes to (via `inputSink` →
/// `MacGhosttyTerminalView.writeToPty` → `ghostty_surface_text`). Kill tears the
/// session terminal down.
///
/// PTY-tee constraint (macOS): libghostty owns the child's PTY (it spawns
/// `config.command`) and exposes NO output-read API — so `feed(_:)` is driven by
/// a spawn-level tee (the launcher runs `claude` under `script`, and a file tail
/// pushes bytes here). See MacCodingLauncher + SteerPtyTail.
@MainActor
final class MacSteerPublisher {
    let sessionId: String
    let issueId: String

    private let accountId: String
    private let steerApi: SteerApi
    /// Writes a remote steerer's keystrokes into the session PTY. Called on main.
    private let inputSink: @MainActor (String) -> Void
    /// Kill-switch: tear the session terminal down (surface destroy kills claude).
    private let onKill: @MainActor () -> Void

    private var task: URLSessionWebSocketTask?
    private var connected = false
    private var stopped = false

    // Bounded replay buffer. We can't read ghostty's visible grid, so the most
    // recent output bytes are the pragmatic `resync` payload (claude's own redraws
    // repaint the screen for a mid-session viewer).
    private var ring = Data()
    private let ringCap = 256 * 1024

    // Backpressure: cap outstanding output sends; coalesce (drop output frames)
    // past the cap. Control frames are NEVER dropped.
    private var inFlight = 0
    private let inFlightCap = 32

    private var cols = 80
    private var rows = 24

    init(
        sessionId: String,
        issueId: String,
        accountId: String,
        steerApi: SteerApi,
        inputSink: @escaping @MainActor (String) -> Void,
        onKill: @escaping @MainActor () -> Void
    ) {
        self.sessionId = sessionId
        self.issueId = issueId
        self.accountId = accountId
        self.steerApi = steerApi
        self.inputSink = inputSink
        self.onKill = onKill
    }

    func start() { Task { await connect() } }

    private func connect() async {
        guard !stopped else { return }
        let ticket: SteerTicket
        do {
            ticket = try await steerApi.mintPublisherTicket(accountId: accountId, codingSessionId: sessionId)
        } catch {
            return // relay unreachable — steering is purely additive, so just skip
        }
        guard !ticket.isDisabled, let url = ticket.connectURL() else { return }
        let t = URLSession.shared.webSocketTask(with: url)
        task = t
        t.resume()
        connected = true
        sendText(SteerOutbound.hello(sessionId: sessionId, issueId: issueId, cols: cols, rows: rows))
        replay() // hand the relay whatever we've already buffered locally
        receiveLoop()
    }

    private func receiveLoop() {
        // The completion runs off the main actor with a `Result<Message, any Error>`
        // (not Sendable), so extract the Sendable String / outcome here and hop
        // to the main actor with only that.
        task?.receive { [weak self] result in
            switch result {
            case .success(.string(let text)):
                Task { @MainActor in self?.onText(text) }
            case .success:
                Task { @MainActor in self?.rearm() } // binary inbound not expected
            case .failure:
                Task { @MainActor in self?.onFailure() }
            }
        }
    }

    private func onText(_ text: String) {
        if let frame = SteerInbound.decode(text) { handle(frame) }
        rearm()
    }

    private func rearm() {
        if !stopped { receiveLoop() }
    }

    private func onFailure() {
        // The socket dropped; the relay marks the room stale. v1 does not
        // auto-reconnect the publisher (the session keeps running locally).
        // TODO(Phase 6): re-`hello` on reconnect and resume the same room.
        connected = false
    }

    private func handle(_ frame: SteerInbound) {
        switch frame {
        case let .input(data):
            inputSink(data) // → ghostty_surface_text → the session PTY
        case .resync:
            replay()
        case .kill:
            onKill()
            stop(outcome: "killed")
        case .startSession, .presence, .unknown:
            break
        }
    }

    /// Feed teed PTY output: buffer it and forward a binary frame (drop on
    /// overflow so a slow relay/viewer never blocks the producer).
    func feed(_ data: Data) {
        guard !data.isEmpty else { return }
        appendRing(data)
        guard connected, inFlight < inFlightCap else { return }
        sendBinary(SteerOutbound.outputFrame(data))
    }

    /// The local terminal resized — tell viewers to reflow.
    func sendResize(cols: Int, rows: Int) {
        self.cols = cols
        self.rows = rows
        sendText(SteerOutbound.resize(cols: cols, rows: rows))
    }

    /// End the room. Idempotent.
    func stop(outcome: String) {
        guard !stopped else { return }
        stopped = true
        sendText(SteerOutbound.bye(outcome: outcome))
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        connected = false
    }

    // MARK: - Send helpers

    private func replay() {
        guard connected, !ring.isEmpty, inFlight < inFlightCap else { return }
        sendBinary(SteerOutbound.outputFrame(ring))
    }

    private func appendRing(_ data: Data) {
        ring.append(data)
        if ring.count > ringCap { ring.removeFirst(ring.count - ringCap) }
    }

    private func sendText(_ text: String) {
        guard connected, let task else { return }
        task.send(.string(text)) { _ in }
    }

    private func sendBinary(_ data: Data) {
        guard connected, let task else { return }
        inFlight += 1
        task.send(.data(data)) { [weak self] _ in
            Task { @MainActor in self?.decrementInFlight() }
        }
    }

    private func decrementInFlight() {
        inFlight = max(0, inFlight - 1)
    }
}
