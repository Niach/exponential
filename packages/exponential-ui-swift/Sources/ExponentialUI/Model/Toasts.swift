import SwiftUI
import ExponentialUICore

/// One running toast timer (the remaining time survives a pause).
struct ToastTimer {
    var remainingMs: Double
    var started: ContinuousClock.Instant?
    var task: Task<Void, Never>?
}

/// Toast timers (round-1 contract §3 Toast): the model runs each open
/// toast's `duration` (0 = sticky), PAUSED while it is hovered or holds
/// focus; on expiry `dismissToast` (the core fires `dismiss` + `change` and
/// writes `open: false`). A toast never takes focus.
extension SurfaceModel {
    /// Start a timer for every new timed toast; forget the gone ones.
    func syncToasts() {
        let open = Set(toasts.map(\.id))
        for id in Array(toastTimers.keys) where !open.contains(id) {
            toastTimers[id]?.task?.cancel()
            toastTimers[id] = nil
            toastHeld[id] = nil
        }
        for t in toasts where t.durationMs > 0 && toastTimers[t.id] == nil {
            toastTimers[t.id] = ToastTimer(remainingMs: t.durationMs)
            if toastHeld[t.id]?.isEmpty ?? true { toastResume(t.id) }
        }
    }

    /// The pointer over a toast (any of its nodes) pauses its timer.
    public func toastHover(id: String, _ hovered: Bool) {
        if hovered { pointerSeen() }
        toastHold(toastOwner(of: id) ?? id, "hover", hovered)
    }

    /// Is this toast's timer paused (hovered or focused)?
    public func toastPaused(_ id: String) -> Bool { !(toastHeld[id]?.isEmpty ?? true) }

    /// The milliseconds a timed toast has left (nil = sticky / unknown).
    public func toastRemainingMs(_ id: String) -> Double? {
        guard let t = toastTimers[id] else { return nil }
        guard let s = t.started else { return t.remainingMs }
        let elapsed = ContinuousClock.now - s
        return max(0, t.remainingMs - Double(elapsed.components.seconds) * 1000 - Double(elapsed.components.attoseconds) / 1e15)
    }

    /// Hold (or release) a toast's timer for a reason (`hover`, `focus`).
    func toastHold(_ id: String, _ reason: String, _ on: Bool) {
        var held = toastHeld[id] ?? []
        if on { held.insert(reason) } else { held.remove(reason) }
        toastHeld[id] = held
        if held.isEmpty { toastResume(id) } else { toastPause(id) }
    }

    /// The Toast a node belongs to (the node itself, a part, or a descendant).
    func toastOwner(of id: String) -> String? {
        var cur = byId[id]
        var guardCount = 0
        while let i = cur, let n = node(i), guardCount < 4096 {
            if n.component == "Toast" { return n.id }
            if n.ownerComponent == "Toast" { return n.owner }
            cur = n.parent
            guardCount += 1
        }
        return nil
    }

    private func toastPause(_ id: String) {
        guard var t = toastTimers[id], let started = t.started else { return }
        let elapsed = ContinuousClock.now - started
        t.remainingMs = max(0, t.remainingMs - (Double(elapsed.components.seconds) * 1000 + Double(elapsed.components.attoseconds) / 1e15))
        t.started = nil
        t.task?.cancel()
        t.task = nil
        toastTimers[id] = t
    }

    private func toastResume(_ id: String) {
        guard var t = toastTimers[id], t.task == nil else { return }
        let remaining = t.remainingMs
        t.started = .now
        t.task = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(remaining.rounded(.up))))
            guard !Task.isCancelled, let self else { return }
            self.toastExpired(id)
        }
        toastTimers[id] = t
    }

    private func toastExpired(_ id: String) {
        toastTimers[id] = nil
        toastHeld[id] = nil
        dispatch(surface.dismissToast(id: id))
    }

    /// Dismiss a toast now (its close button does this through the core).
    public func dismissToast(_ id: String) {
        toastTimers[id]?.task?.cancel()
        toastTimers[id] = nil
        dispatch(surface.dismissToast(id: id))
    }
}
