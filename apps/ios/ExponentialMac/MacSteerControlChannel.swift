import AppKit
import ExpCore
import Foundation

/// The desktop's outbound device-presence socket (masterplan §3.2). While the
/// app is open it holds one control socket per signed-in account (when the relay
/// is configured), announcing device presence so the phone's "Start on my
/// desktop" picker can see this Mac, and routing an inbound `start_session` to
/// the native launcher. Graceful-off: if `steer.config` reports disabled, no
/// socket is opened. Purely additive — never load-bearing for local coding.
@MainActor
@Observable
final class MacSteerControlChannel {
    private let auth: AuthRepository
    private let steerApi: SteerApi
    private let settings: MacCodingSettings
    /// Route a remote start to the launcher: (accountId, issueId).
    private let onStartSession: @MainActor (String, String) -> Void

    private var connections: [String: ControlConnection] = [:] // accountId → conn
    private var reconcileTask: Task<Void, Never>?

    init(
        auth: AuthRepository,
        steerApi: SteerApi,
        settings: MacCodingSettings,
        onStartSession: @escaping @MainActor (String, String) -> Void
    ) {
        self.auth = auth
        self.steerApi = steerApi
        self.settings = settings
        self.onStartSession = onStartSession
    }

    func start() {
        guard reconcileTask == nil else { return }
        reconcileTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                self?.reconcile()
                try? await Task.sleep(for: .seconds(3))
            }
        }
    }

    func stop() {
        reconcileTask?.cancel()
        reconcileTask = nil
        for (_, conn) in connections { conn.close() }
        connections.removeAll()
    }

    /// Ensure exactly one control connection per signed-in account.
    private func reconcile() {
        let signedIn = Set(auth.accounts.filter { $0.token != nil }.map { $0.id })
        for id in connections.keys where !signedIn.contains(id) {
            connections[id]?.close()
            connections[id] = nil
        }
        for id in signedIn where connections[id] == nil {
            let conn = ControlConnection(
                accountId: id, steerApi: steerApi, deviceId: settings.deviceId,
                onStartSession: { [weak self] issueId in self?.onStartSession(id, issueId) }
            )
            connections[id] = conn
            conn.start()
        }
    }
}

/// One account's control socket, with config-gated connect + reconnect backoff.
@MainActor
private final class ControlConnection {
    private let accountId: String
    private let steerApi: SteerApi
    private let deviceId: String
    private let onStartSession: @MainActor (String) -> Void

    private var task: URLSessionWebSocketTask?
    private var stopped = false
    private var backoff: Double = 2

    init(accountId: String, steerApi: SteerApi, deviceId: String, onStartSession: @escaping @MainActor (String) -> Void) {
        self.accountId = accountId
        self.steerApi = steerApi
        self.deviceId = deviceId
        self.onStartSession = onStartSession
    }

    func start() { attempt() }

    func close() {
        stopped = true
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    private func attempt() {
        guard !stopped else { return }
        Task { @MainActor in
            // Config gate: only dial when the relay is configured on this instance.
            guard let config = try? await steerApi.config(accountId: accountId), config.enabled,
                  let ticket = try? await steerApi.mintControlTicket(
                      accountId: accountId, deviceLabel: Self.deviceLabel),
                  !ticket.isDisabled, let url = ticket.connectURL()
            else {
                scheduleReconnect()
                return
            }
            guard !stopped else { return }
            let t = URLSession.shared.webSocketTask(with: url)
            task = t
            t.resume()
            backoff = 2
            sendOnline()
            receiveLoop()
        }
    }

    private func receiveLoop() {
        // Extract the Sendable String in the (non-isolated) completion, then hop —
        // `Result<Message, any Error>` isn't Sendable.
        task?.receive { [weak self] result in
            switch result {
            case .success(.string(let text)):
                Task { @MainActor in self?.onText(text) }
            case .success:
                Task { @MainActor in self?.rearm() }
            case .failure:
                Task { @MainActor in self?.scheduleReconnect() }
            }
        }
    }

    private func onText(_ text: String) {
        if let frame = SteerInbound.decode(text),
           case let .startSession(issueId) = frame,
           !issueId.isEmpty {
            onStartSession(issueId)
        }
        rearm()
    }

    private func rearm() {
        if !stopped { receiveLoop() }
    }

    private func scheduleReconnect() {
        guard !stopped else { return }
        task = nil
        let delay = backoff
        backoff = min(backoff * 2, 30)
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            self?.attempt()
        }
    }

    private func sendOnline() {
        task?.send(.string(SteerOutbound.online(deviceId: deviceId, deviceLabel: Self.deviceLabel))) { _ in }
    }

    static var deviceLabel: String {
        Host.current().localizedName ?? ProcessInfo.processInfo.hostName
    }
}
