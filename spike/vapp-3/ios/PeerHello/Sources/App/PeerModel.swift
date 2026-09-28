import Foundation
import Network
import SwiftUI

/// Drives one PeerLink against the VAPP-3 daemon through the steer relay (viewer role).
/// Relay transport (README): viewer → daemon `{"t":"input","data":"<envelope>"}`, daemon → viewers
/// `{"t":"activity","event":{"kind":"narration","text":"<envelope>"}}`.
@MainActor
final class PeerModel: ObservableObject {
    static let policies = ["all", "relay", "host"]
    static let scenarios = ["same-lan", "home-wifi-cellular", "home-office-wifi", "cellular-cellular"]

    // Inputs (launch arguments `-relay`, `-ticket`, `-session`, `-turn`, `-policy`, `-scenario` override).
    @Published var relayURL: String
    @Published var ticket: String
    @Published var sessionId: String
    @Published var turnHost: String
    @Published var turnUser = "exp"
    @Published var turnPass = "spike"
    @Published var turnRealm = "exponential.local"
    @Published var policy: String
    /// Launch argument `-stun host:port`: a separate STUN server (public one for the cellular rows).
    var stunHost: String?
    @Published var scenario: String

    // State.
    @Published var relayStatus = "idle"
    @Published var daemonId: String?
    @Published var linkState = "-"
    @Published var connectMs: Double?
    @Published var benchSummary: String?
    @Published var resultJSON = ""
    @Published var diagnostics = ""
    @Published var log: [String] = []
    @Published var lastReconnect: (trigger: String, ms: Double?)?
    let coreVersion = peerVersion()

    #if targetEnvironment(simulator)
    let source = "ios-sim"
    #else
    let source = "ios-device"
    #endif

    private let peerId = "ios-" + UUID().uuidString.prefix(8).lowercased()
    private var ws: URLSessionWebSocketTask?
    private var wsGeneration = 0
    private var daemonPub: String?
    private var link: PeerLink?
    private var pumpStop = StopFlag()
    private var lastBench: BenchResult?
    private var lastError: String?
    private var helloWaiters: [CheckedContinuation<Void, Never>] = []
    private var answerCount = 0
    private var replayDone = false
    private var wasBackgrounded = false
    private let pathMonitor = NWPathMonitor()
    private var lastPathKey: String?
    private var reconnecting = false
    private var autoStarted = false
    private let linkQueue = DispatchQueue(label: "peerhello.link", qos: .userInitiated)

    init() {
        let d = UserDefaults.standard
        let mac = DevTickets.macHost
        relayURL = d.string(forKey: "relay") ?? "ws://\(mac):4002"
        ticket = d.string(forKey: "ticket") ?? DevTickets.viewer
        sessionId = d.string(forKey: "session") ?? DevTickets.sessionId
        turnHost = d.string(forKey: "turn") ?? "\(mac):3478"
        stunHost = d.string(forKey: "stun")
        policy = d.string(forKey: "policy") ?? "all"
        #if targetEnvironment(simulator)
        scenario = d.string(forKey: "scenario") ?? "same-lan"
        #else
        scenario = d.string(forKey: "scenario") ?? "same-lan"
        #endif
        startPathMonitor()
    }

    // MARK: - Logging

    private func say(_ s: String) {
        let line = "\(Self.clock()) \(s)"
        log.append(line)
        if log.count > 400 { log.removeFirst(log.count - 400) }
        print("PEERHELLO \(line)")
    }

    private static func clock() -> String {
        let f = DateFormatter(); f.dateFormat = "HH:mm:ss.SSS"; return f.string(from: Date())
    }

    private static func iso() -> String {
        let f = ISO8601DateFormatter(); f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]; return f.string(from: Date())
    }

    // MARK: - Relay

    private func openRelay() {
        ws?.cancel(with: .goingAway, reason: nil)
        wsGeneration += 1
        let gen = wsGeneration
        var base = relayURL.trimmingCharacters(in: .whitespaces)
        if !base.contains("/ws") { base += "/ws" }
        guard let url = URL(string: "\(base)?ticket=\(ticket.trimmingCharacters(in: .whitespaces))") else {
            relayStatus = "bad url"; return
        }
        let task = URLSession.shared.webSocketTask(with: url)
        task.maximumMessageSize = 1 << 20
        ws = task
        task.resume()
        relayStatus = "connecting"
        sendFrame(["t": "join", "channel": "activity"])
        receive(task, gen)
    }

    private func receive(_ task: URLSessionWebSocketTask, _ gen: Int) {
        task.receive { [weak self] result in
            Task { @MainActor in
                guard let self, gen == self.wsGeneration else { return }
                switch result {
                case .failure(let e):
                    self.relayStatus = "closed: \(e.localizedDescription)"
                    self.say("relay closed: \(e.localizedDescription)")
                case .success(let msg):
                    if case .string(let s) = msg { self.handleFrame(s) }
                    else if case .data(let d) = msg, let s = String(data: d, encoding: .utf8) { self.handleFrame(s) }
                    self.receive(task, gen)
                }
            }
        }
    }

    private func sendFrame(_ obj: [String: Any]) {
        guard let ws, let data = try? JSONSerialization.data(withJSONObject: obj), let s = String(data: data, encoding: .utf8) else { return }
        ws.send(.string(s)) { [weak self] err in
            if let err { Task { @MainActor in self?.say("relay send failed: \(err.localizedDescription)") } }
        }
    }

    /// Envelope to the daemon; stamps `to` when the core left it empty.
    private func sendEnvelope(_ json: String) {
        var out = json
        if let d = daemonId, var obj = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any], obj["to"] == nil {
            obj["to"] = d
            if let data = try? JSONSerialization.data(withJSONObject: obj), let s = String(data: data, encoding: .utf8) { out = s }
        }
        sendFrame(["t": "input", "data": out])
    }

    private func handleFrame(_ s: String) {
        guard let obj = (try? JSONSerialization.jsonObject(with: Data(s.utf8))) as? [String: Any], let t = obj["t"] as? String else { return }
        switch t {
        case "activity":
            if relayStatus != "joined" { relayStatus = "joined" }
            guard let ev = obj["event"] as? [String: Any], let text = ev["text"] as? String else { return }
            handleEnvelope(text)
        case "activity_reset":
            relayStatus = "joined"
            replayDone = false
        case "activity_synced":
            relayStatus = "joined"
            replayDone = true
            say("relay replay done; daemon=\(daemonId ?? "none yet")")
            if daemonId != nil { resumeHelloWaiters() }
        case "error":
            relayStatus = "error: \(obj["message"] ?? obj["code"] ?? "")"
            say("relay error frame: \(s.prefix(200))")
            resumeHelloWaiters()
        default:
            break
        }
    }

    private func handleEnvelope(_ text: String) {
        guard let env = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any],
              let type = env["type"] as? String, let from = env["from"] as? String else { return }
        if let to = env["to"] as? String, to != peerId { return }
        switch type {
        case "hello":
            // Keep the LAST hello (the replay can hold older daemon incarnations).
            if daemonId != from { say("hello from daemon \(from) \(env["version"] ?? "?")\(replayDone ? "" : " (replay)")") }
            daemonId = from
            daemonPub = env["pub"] as? String
            if replayDone { resumeHelloWaiters() }
        case "answer":
            guard from == daemonId, let link else { return }
            answerCount += 1
            say("answer received (\(answerCount))")
            linkQueue.async {
                do { try link.acceptAnswer(envelopeJson: text) }
                catch { Task { @MainActor in self.fail("acceptAnswer: \(error)") } }
            }
        case "candidate", "end_of_candidates":
            guard from == daemonId, let link else { return }
            linkQueue.async { try? link.addRemoteCandidate(candidate: text) }
        case "reject":
            say("daemon rejected: \(env["reason"] ?? "")")
            lastError = "rejected: \(env["reason"] ?? "")"
        default:
            break
        }
    }

    private func resumeHelloWaiters() {
        let w = helloWaiters; helloWaiters = []
        w.forEach { $0.resume() }
    }

    private func waitForHello(seconds: Double) async -> Bool {
        if daemonId != nil && replayDone { return true }
        let timeout = Task { @MainActor in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
            self.resumeHelloWaiters()
        }
        await withCheckedContinuation { helloWaiters.append($0) }
        timeout.cancel()
        return daemonId != nil
    }

    private func fail(_ s: String) {
        lastError = s
        say("ERROR \(s)")
    }

    // MARK: - Actions

    func connect() { Task { await connectFlow() } }

    @discardableResult
    func connectFlow() async -> Bool {
        close()
        lastError = nil; connectMs = nil; answerCount = 0; daemonId = nil; daemonPub = nil; replayDone = false
        lastBench = nil; lastReconnect = nil
        openRelay()
        say("joining \(sessionId) as \(peerId)")
        guard await waitForHello(seconds: 10) else { fail("no daemon hello within 10s"); return false }
        let cfg = PeerConfig(
            peerId: peerId, sessionId: sessionId,
            stun: stunHost ?? (turnHost.isEmpty || turnHost == "none" ? nil : turnHost),
            turn: turnHost.isEmpty || turnHost == "none" ? nil : turnHost,
            turnUsername: turnUser, turnPassword: turnPass, turnRealm: turnRealm,
            policy: policy == "relay" ? .relay : policy == "host" ? .host : .all,
            identitySeed: nil, expectedRemotePubkey: daemonPub,
            tamperSdp: false, tamperSig: false)
        let t0 = Date()
        let link: PeerLink
        do { link = try PeerLink(config: cfg) } catch { fail("PeerLink.new: \(error)"); return false }
        self.link = link
        let offer: String
        do { offer = try await onLinkQueue { try link.createOffer() } } catch { fail("createOffer: \(error)"); return false }
        sendEnvelope(offer)
        say("offer sent")
        startPump(link)
        let ok = await waitForState(link, .connected, seconds: 30)
        if ok {
            connectMs = Date().timeIntervalSince(t0) * 1000
            say(String(format: "connected in %.0f ms", connectMs!))
        } else {
            fail("not connected within 30s (state \(linkState))")
        }
        return ok
    }

    private func onLinkQueue<T>(_ body: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { cont in
            linkQueue.async { cont.resume(with: Result { try body() }) }
        }
    }

    private func startPump(_ link: PeerLink) {
        let flag = StopFlag()
        pumpStop = flag
        Thread.detachNewThread { [weak self] in
            var lastUI = Date.distantPast
            while !flag.isSet {
                let out = link.pollSignals(timeoutMs: 200)
                let now = Date()
                let refresh = now.timeIntervalSince(lastUI) > 0.5
                let st = refresh ? Self.name(link.state()) : nil
                let diag = refresh ? link.diagnostics() : nil
                if refresh { lastUI = now }
                if out.isEmpty && !refresh { continue }
                DispatchQueue.main.async {
                    guard let self else { return }
                    for e in out { self.sendEnvelope(e) }
                    if let st { self.linkState = st }
                    if let diag { self.diagnostics = diag }
                }
            }
        }
    }

    nonisolated static func name(_ s: LinkState) -> String {
        switch s {
        case .new: return "new"
        case .connecting: return "connecting"
        case .connected: return "connected"
        case .disconnected: return "disconnected"
        case .failed: return "failed"
        case .closed: return "closed"
        }
    }

    private func waitForState(_ link: PeerLink, _ want: LinkState, seconds: Double, after: (() -> Bool)? = nil) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            let s = link.state()
            linkState = Self.name(s)
            if s == want && (after?() ?? true) { return true }
            if s == .failed || s == .closed { return false }
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        return false
    }

    func bench() { Task { await benchFlow() } }

    @discardableResult
    func benchFlow() async -> Bool {
        guard let link else { fail("bench: not connected"); return false }
        say("bench 2 MB × up/down + 100 pings…")
        do {
            let r = try await onLinkQueueDetached { try link.runBench(bodyBytes: 2_000_000, pings: 100) }
            lastBench = r
            benchSummary = String(format: "connect %.0f ms · rtt p50 %.1f / p95 %.1f ms\nup %.1f Mbps · down %.1f Mbps\n%@ ↔ %@", r.connectMs, r.rttP50Ms, r.rttP95Ms, r.upMbps, r.downMbps, r.localPath, r.remotePath)
            if !r.errors.isEmpty { lastError = r.errors.joined(separator: "; ") }
            say("bench done: \(benchSummary!.replacingOccurrences(of: "\n", with: " | "))")
            buildResult()
            return true
        } catch {
            fail("runBench: \(error)")
            buildResult()
            return false
        }
    }

    /// Bench blocks for seconds: keep it off the link queue so answers/candidates still apply.
    private func onLinkQueueDetached<T>(_ body: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async { cont.resume(with: Result { try body() }) }
        }
    }

    func reconnect(trigger: String) { Task { await reconnectFlow(trigger: trigger) } }

    func reconnectFlow(trigger: String) async {
        guard let link, !reconnecting else { return }
        reconnecting = true
        defer { reconnecting = false }
        say("reconnect (\(trigger)): ICE restart")
        let t0 = Date()
        if ws == nil || ws?.state != .running || relayStatus.hasPrefix("closed") {
            openRelay()
            _ = await waitForHello(seconds: 5)
        }
        let answersBefore = answerCount
        do { try await onLinkQueue { try link.restartIce() } } catch {
            fail("restartIce: \(error)"); lastReconnect = (trigger, nil); buildResult(); return
        }
        let ok = await waitForState(link, .connected, seconds: 30, after: { [weak self] in (self?.answerCount ?? 0) > answersBefore })
        let ms = ok ? Date().timeIntervalSince(t0) * 1000 : nil
        lastReconnect = (trigger, ms)
        say(ok ? String(format: "reconnected in %.0f ms", ms!) : "reconnect failed (state \(linkState))")
        buildResult()
    }

    func close() {
        pumpStop.set()
        if let link { linkQueue.async { link.close() } }
        link = nil
        ws?.cancel(with: .normalClosure, reason: nil)
        ws = nil
        wsGeneration += 1
        relayStatus = "idle"
        linkState = "-"
    }

    // MARK: - Results

    @discardableResult
    func buildResult() -> String {
        let r = lastBench
        var obj: [String: Any] = [
            "kind": "bench", "at": Self.iso(), "source": source, "scenario": scenario, "policy": policy,
            "connectMs": r?.connectMs ?? connectMs ?? 0,
            "rttP50Ms": r?.rttP50Ms ?? 0, "rttP95Ms": r?.rttP95Ms ?? 0,
            "upMbps": r?.upMbps ?? 0, "downMbps": r?.downMbps ?? 0,
            "bodyBytes": r?.bodyBytes ?? 2_000_000,
            "localPath": r?.localPath ?? "", "remotePath": r?.remotePath ?? "",
            "ok": r != nil && lastError == nil,
            "error": lastError ?? NSNull(),
            "reconnect": lastReconnect.map { ["trigger": $0.trigger, "ms": $0.ms ?? NSNull()] as [String: Any] } ?? NSNull(),
            "notes": "PeerHello \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] ?? "") core \(coreVersion); appConnectMs \(connectMs.map { String(format: "%.0f", $0) } ?? "-"); local \(r?.localCandidate ?? "-"); remote \(r?.remoteCandidate ?? "-")",
        ]
        if obj["error"] is NSNull, let r, !r.errors.isEmpty { obj["error"] = r.errors.joined(separator: "; ") }
        let data = (try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys])) ?? Data()
        resultJSON = String(data: data, encoding: .utf8) ?? ""
        return resultJSON
    }

    func sendResults() { Task { await sendResultsFlow() } }

    func sendResultsFlow() async {
        let json = resultJSON.isEmpty ? buildResult() : resultJSON
        print("PEERHELLO_RESULT \(json)")
        if ws != nil {
            sendEnvelope(resultEnvelope(from: peerId, to: daemonId, sessionId: sessionId, payloadJson: json))
            say("result sent through the relay")
        }
        // Fallback collector on the Mac (web/serve.ts), best effort.
        if let url = URL(string: "http://\(DevTickets.macHost):8787/results") {
            var req = URLRequest(url: url, timeoutInterval: 3)
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "content-type")
            req.httpBody = Data(json.utf8)
            do {
                let (_, resp) = try await URLSession.shared.data(for: req)
                say("result POSTed to :8787 (\((resp as? HTTPURLResponse)?.statusCode ?? 0))")
            } catch {
                say("POST :8787 unreachable (\(error.localizedDescription))")
            }
        }
    }

    // MARK: - Auto mode + lifecycle triggers

    func startAutoIfRequested() {
        guard !autoStarted, UserDefaults.standard.bool(forKey: "auto") else { return }
        autoStarted = true
        Task {
            say("auto: connect → bench → send")
            if await connectFlow() { await benchFlow() } else { buildResult() }
            if UserDefaults.standard.bool(forKey: "autoReconnect"), link != nil {
                await reconnectFlow(trigger: "manual")
            }
            await sendResultsFlow()
            print("PEERHELLO_DONE")
        }
    }

    func scenePhaseChanged(_ phase: ScenePhase) {
        switch phase {
        case .background: wasBackgrounded = true
        case .active:
            if wasBackgrounded, link != nil {
                if UserDefaults.standard.bool(forKey: "autoAfterReconnect") {
                    // Scripted background test: reconnect, then bench + send without a tap.
                    Task { await reconnectFlow(trigger: "background"); await benchFlow(); await sendResultsFlow() }
                } else {
                    reconnect(trigger: "background")
                }
            }
            wasBackgrounded = false
        default: break
        }
    }

    private func startPathMonitor() {
        pathMonitor.pathUpdateHandler = { [weak self] path in
            let key = path.availableInterfaces.map { "\($0.type)" }.joined(separator: ",") + "|\(path.status)"
            Task { @MainActor in
                guard let self else { return }
                defer { self.lastPathKey = key }
                guard let prev = self.lastPathKey, prev != key, path.status == .satisfied, self.link != nil else { return }
                self.say("network changed: \(prev) → \(key)")
                self.reconnect(trigger: "network-switch")
            }
        }
        pathMonitor.start(queue: DispatchQueue(label: "peerhello.path"))
    }
}

/// One per pump thread, so a stale pump never outlives its link.
final class StopFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false
    var isSet: Bool { lock.lock(); defer { lock.unlock() }; return value }
    func set() { lock.lock(); value = true; lock.unlock() }
}
