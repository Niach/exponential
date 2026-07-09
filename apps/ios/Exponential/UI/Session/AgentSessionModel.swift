import ExpCore
import Foundation
import GRDB

/// One watcher in the relay room (relay `presence` frames).
struct AgentPresenceViewer: Identifiable, Equatable {
    let userId: String
    let name: String
    let perm: String
    var id: String { userId }
}

/// Viewer side of the steer relay's ACTIVITY channel (EXP-32 — the chat-style
/// "Agent session" screen; apps/steer-relay/src/protocol.ts). Mints a viewer
/// ticket over tRPC, dials the returned ws(s) URL with URLSessionWebSocketTask,
/// joins with {"t":"join","channel":"activity"}, and receives scrubbed
/// {t:'activity', event} frames (narration / tool headlines / worktree diffs)
/// instead of raw PTY bytes. TEXT frames are JSON control messages; stray
/// BINARY frames (0x01 PTY output, a relay/desktop misroute) are ignored.
/// Steering is message-shaped: a steal-claim + chunked input + a separate \r.
/// Mirrors the Android AgentSessionViewModel.
@MainActor @Observable
final class AgentSessionModel {
    enum Phase: Equatable {
        case idle
        case connecting
        case live
        /// The relay reported no_such_session while the synced row still says
        /// running — the desktop is still dialing its publisher socket. The
        /// model auto-redials (fresh ticket) every ~3s until the room is live.
        case starting
        /// The session ended (relay `bye`). Feed retained, input hidden.
        case ended(detail: String?)
        /// Unexpected socket loss / ticket failure — Reconnect offered.
        case closed(detail: String?)
    }

    /// One rendered feed entry. Diffs never enter the feed — see `latestDiff`.
    enum FeedItem: Identifiable, Equatable {
        case narration(id: Int, text: String)
        case tool(id: Int, name: String, detail: String?)

        var id: Int {
            switch self {
            case let .narration(id, _): id
            case let .tool(id, _, _): id
            }
        }
    }

    private(set) var phase: Phase = .idle
    /// The feed stays visible while disconnected (closed/ended states) but is
    /// cleared right before each rejoin — the relay replays the room's whole
    /// activity log to every joining socket, so keeping it would duplicate
    /// the entire history on reconnect.
    private(set) var feed: [FeedItem] = []
    /// The most recent worktree diff — each one replaces the previous.
    private(set) var latestDiff: String?
    private(set) var viewers: [AgentPresenceViewer] = []
    private(set) var steererId: String?
    /// The minted ticket's perm claim (`view`/`steer`), display-gating only —
    /// the relay enforces it server-side regardless.
    private(set) var perm = "view"
    /// The synced coding_sessions row — flips to ended via Electric.
    private(set) var session: CodingSessionEntity?

    var canSteer: Bool { perm == "steer" }
    var isSteering: Bool { steererId != nil && steererId == currentUserId }
    var remoteSteererName: String? {
        guard let steererId, steererId != currentUserId else { return nil }
        return viewers.first { $0.userId == steererId }?.name ?? "Someone"
    }
    var sessionEnded: Bool { session?.status == DomainContract.codingSessionStatusEnded }

    private let accountId: String
    private let codingSessionId: String
    private let currentUserId: String?
    private let steerApi: SteerApi
    private let db: DatabaseManager

    private var task: URLSessionWebSocketTask?
    private var connected = false
    private var stopped = false
    private var sawEnd = false
    private var retryStarting = false
    private var endDetail: String?
    private var nextEventId = 0
    private var retryTask: Task<Void, Never>?
    private var idleReleaseTask: Task<Void, Never>?
    private var sessionObservationTask: Task<Void, Never>?

    // Relay rejects input frames > 8 KiB; chunk pastes well under that.
    private static let inputChunkChars = 4096
    /// Client-side feed cap — old events fall off the top.
    private static let feedCap = 500
    /// Auto-release the steer claim after this long with no sends.
    private static let idleReleaseSeconds: Double = 60
    /// Redial cadence while the desktop's publisher socket is still starting.
    private static let startingRetrySeconds: Double = 3

    init(
        accountId: String,
        session: CodingSessionEntity,
        currentUserId: String?,
        steerApi: SteerApi,
        db: DatabaseManager
    ) {
        self.accountId = accountId
        self.codingSessionId = session.id
        self.currentUserId = currentUserId
        self.steerApi = steerApi
        self.db = db
        self.session = session
    }

    /// Bind the synced session row and auto-connect once when presented;
    /// reconnects after that are explicit.
    func start() {
        startObservingSession()
        if phase == .idle { connect() }
    }

    /// Dial (or re-dial, with a fresh ticket) the relay room.
    func connect() {
        guard phase != .connecting, phase != .live else { return }
        stopped = false
        retryTask?.cancel()
        resetDialState()
        phase = .connecting
        Task { await dial() }
    }

    /// Tear everything down (view dismissed): best-effort claim release, then
    /// close. Closing the socket also releases the claim relay-side.
    func shutdown() {
        stopped = true
        releaseNow()
        retryTask?.cancel()
        idleReleaseTask?.cancel()
        sessionObservationTask?.cancel()
        sessionObservationTask = nil
        connected = false
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    // MARK: - Steering (message-shaped; the relay enforces the single claim)

    /// Send one message to the agent: ALWAYS steal-claim first (the relay
    /// gates input per CONNECTION while presence only exposes the steerer's
    /// USER id — if this user holds the claim on another socket, e.g. the web
    /// steer terminal or a second phone, skipping the claim would make the
    /// relay silently drop every frame; the claim is idempotent for the
    /// current holder and last-writer-wins by design), then forward the text
    /// (chunked ≤4 KiB), then a SEPARATE `\r` frame — bundled into one write
    /// TUI apps treat the trailing return as a paste, which inserts instead
    /// of submitting.
    func sendMessage(_ text: String) {
        guard !text.isEmpty, canSteer, connected else { return }
        sendText(#"{"t":"claim","steal":true}"#)
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
        sendText(#"{"t":"input","data":"\r"}"#)
        scheduleIdleRelease()
    }

    /// Best-effort claim release — closing the socket also releases it
    /// relay-side; this just makes it prompt.
    func releaseNow() {
        idleReleaseTask?.cancel()
        guard isSteering else { return }
        sendText(#"{"t":"release"}"#)
    }

    /// Auto-release the claim after 60s of no sends (timer resets per send).
    private func scheduleIdleRelease() {
        idleReleaseTask?.cancel()
        idleReleaseTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.idleReleaseSeconds))
            guard !Task.isCancelled else { return }
            self?.releaseNow()
        }
    }

    // MARK: - Synced session row

    private func startObservingSession() {
        guard sessionObservationTask == nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let id = codingSessionId
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity.filter(Column("id") == id).fetchOne(db)
        }
        sessionObservationTask = Task { [weak self] in
            do {
                for try await row in observation.values(in: pool) {
                    guard let self, let row else { continue }
                    self.session = row
                }
            } catch {}
        }
    }

    // MARK: - Connect lifecycle

    private func resetDialState() {
        sawEnd = false
        retryStarting = false
        endDetail = nil
        viewers = []
        steererId = nil
    }

    private func dial() async {
        let ticket: SteerTicket
        do {
            ticket = try await steerApi.mintViewerTicket(accountId: accountId, codingSessionId: codingSessionId)
        } catch {
            guard !stopped else { return }
            phase = .closed(detail: "Couldn't get a viewer ticket. \(error.localizedDescription)")
            return
        }
        guard !stopped else { return }
        guard !ticket.isDisabled, let url = ticket.connectURL() else {
            phase = .closed(detail: "Live sessions are unavailable on this instance.")
            return
        }
        perm = Self.decodeTicketPerm(ticket.ticket ?? "")

        let t = URLSession.shared.webSocketTask(with: url)
        task = t
        connected = true
        t.resume()
        // The relay replays the full activity log (+ latest diff) on join —
        // clear the kept feed now so the replay rebuilds it instead of
        // appending a duplicate copy of the whole history.
        feed = []
        nextEventId = 0
        latestDiff = nil
        sendText(#"{"t":"join","channel":"activity"}"#)
        phase = .live
        receiveLoop()
    }

    private func receiveLoop() {
        // The completion runs off the main actor; extract Sendable payloads and
        // hop back (same shape as the deleted SteerViewerModel's loop).
        task?.receive { [weak self] result in
            switch result {
            case .success(.string(let text)):
                Task { @MainActor in
                    self?.onText(text)
                    self?.rearm()
                }
            case .success:
                // Stray BINARY frame (0x01 PTY output, a relay/desktop
                // misroute) — never render on the activity channel.
                Task { @MainActor in self?.rearm() }
            case .failure:
                Task { @MainActor in self?.onSocketClosed() }
            }
        }
    }

    private func rearm() {
        if !stopped, connected { receiveLoop() }
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
                return AgentPresenceViewer(
                    userId: userId,
                    name: (v["name"] as? String) ?? userId,
                    perm: (v["perm"] as? String) ?? "view"
                )
            }
            steererId = obj["steererId"] as? String
        case "activity":
            handleActivityEvent(obj["event"] as? [String: Any])
        case "bye":
            let outcome = obj["outcome"] as? String
            if outcome == "publisher_lost" {
                // The desktop's relay socket dropped but the session may still
                // be running — the synced row is the truth. Stay retryable
                // (closed, with Reconnect).
                endDetail = "The desktop's connection to the relay dropped — retry once it reconnects."
            } else {
                sawEnd = true
                endDetail = (outcome != nil && outcome != "ended") ? outcome : nil
            }
        case "error":
            let code = (obj["code"] as? String) ?? "error"
            if code == "no_such_session" {
                // Not live on the relay (yet). With the synced row still
                // running this flips into the auto-retrying starting phase.
                retryStarting = true
                endDetail = "The live stream isn't up yet — the desktop may still be connecting."
                disconnectSocket()
                onSocketClosed()
            } else {
                endDetail = (obj["message"] as? String) ?? code
            }
        default:
            break // input/resize/resync/kill — not activity-viewer-relevant
        }
    }

    private func handleActivityEvent(_ event: [String: Any]?) {
        guard let event, let kind = event["kind"] as? String else { return }
        switch kind {
        case "narration":
            guard let text = event["text"] as? String,
                  !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
            append(.narration(id: takeEventId(), text: text))
        case "tool":
            guard let name = event["name"] as? String else { return }
            let detail = (event["detail"] as? String)
                .flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
            append(.tool(id: takeEventId(), name: name, detail: detail))
        case "diff":
            // Diffs never enter the feed — the latest replaces the previous
            // one behind the pinned "Latest changes" chip.
            let diff = event["diff"] as? String
            latestDiff = (diff?.isEmpty == false) ? diff : nil
        default:
            break
        }
    }

    private func takeEventId() -> Int {
        defer { nextEventId += 1 }
        return nextEventId
    }

    private func append(_ item: FeedItem) {
        feed.append(item)
        if feed.count > Self.feedCap {
            feed.removeFirst(feed.count - Self.feedCap)
        }
    }

    private func disconnectSocket() {
        connected = false
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    private func onSocketClosed() {
        guard !stopped else { return }
        connected = false
        task = nil
        idleReleaseTask?.cancel()
        viewers = []
        steererId = nil
        if sawEnd {
            phase = .ended(detail: endDetail)
        } else if retryStarting, session?.status == DomainContract.codingSessionStatusRunning {
            phase = .starting
            scheduleStartingRetry()
        } else {
            phase = .closed(detail: endDetail ?? "Connection lost.")
        }
    }

    /// Redial (fresh ticket) after ~3s while the desktop is still starting —
    /// the phase stays `.starting` so the header doesn't flicker.
    private func scheduleStartingRetry() {
        retryTask?.cancel()
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.startingRetrySeconds))
            guard let self, !self.stopped, !Task.isCancelled else { return }
            if self.sessionEnded {
                self.phase = .ended(detail: nil)
                return
            }
            self.resetDialState()
            await self.dial()
        }
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
