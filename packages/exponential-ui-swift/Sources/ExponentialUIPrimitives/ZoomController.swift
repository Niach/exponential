import SwiftUI
import Observation

/// A settled zoom change (the `zoom` event): the new scale and whether the
/// view is back at its rest (fit) view.
public struct ZoomEvent: Equatable, Sendable {
    public var scale: Double
    public var atFit: Bool

    public init(scale: Double, atFit: Bool) {
        self.scale = scale
        self.atFit = atFit
    }
}

/// A one-shot command from outside the view (a hardware key the host's
/// focus routing caught): applied once per `serial`.
public struct ZoomCommand: Equatable, Sendable {
    public var serial: Int
    /// A zoom key: `+ = - − _ 0 1`.
    public var key: String

    public init(serial: Int, key: String) {
        self.serial = serial
        self.key = key
    }
}

/// EXP-1586: THE state of one `ZoomableImageView`: the view (scale + the
/// picture's offset), the pill, the percent Menu and the gesture bookkeeping.
/// Every number comes from `ZoomMaths` (the core); this object only
/// sequences the input (pinch, drag, momentum, taps, wheel, keys, the pill)
/// and reports settled zoom changes. A custom pill reads it and calls its
/// actions (`zoomIn`, `zoomOut`, `fit`, `select`, `toggleMenu`).
@MainActor
@Observable
public final class ZoomController {
    /// The displayed view (rubber-banded while a gesture holds it).
    public private(set) var view = ZoomView.identity
    public private(set) var frame = ZoomFrame(natural: .zero, viewport: .zero)
    /// The core's rest answer for `frame`.
    public private(set) var rest = ZoomRest(fit: 1, min: 1, max: 1, restScale: 1, view: .identity, tall: false)
    /// THE pill is drawn.
    public private(set) var pillShown = false
    /// The percent button's Menu is open.
    public private(set) var menuOpen = false

    @ObservationIgnored let maths: ZoomMaths
    @ObservationIgnored public var controls: ZoomControls = .auto
    @ObservationIgnored public var reducedMotion = false
    /// The spring back / step animation (the theme's `motion.standard`).
    @ObservationIgnored public var motion: Animation = .timingCurve(0.2, 0, 0, 1, duration: 0.2)
    @ObservationIgnored public var metrics = ZoomPillMetrics.standard
    @ObservationIgnored public var onZoom: ((ZoomEvent) -> Void)?
    @ObservationIgnored public var onTap: (() -> Void)?
    /// The Menu's Fit row (`$string.zoomFit`).
    @ObservationIgnored public var fitLabel = "Fit"

    @ObservationIgnored private var laidOut = false
    @ObservationIgnored private var reported: Double?
    @ObservationIgnored private var gestureStart: ZoomView?
    @ObservationIgnored private var pinching = false
    @ObservationIgnored private var dragAxes = ZoomPanRoom(x: true, y: true)
    @ObservationIgnored private var dragHandedOff = false
    @ObservationIgnored private var momentumTask: Task<Void, Never>?
    @ObservationIgnored private var hideTask: Task<Void, Never>?
    @ObservationIgnored private var wheelTask: Task<Void, Never>?
    @ObservationIgnored private var pillHovered = false

    public init(maths: ZoomMaths) {
        self.maths = maths
    }

    // MARK: derived (all from the core)

    /// The view sits at its rest scale (the fit button is drawn selected).
    public var atRest: Bool { abs(view.scale - rest.restScale) < 1e-4 }
    public var bounds: ZoomBounds { maths.bounds(view, frame) }
    public var percent: String { maths.percent(view.scale) }
    public var panRoom: ZoomPanRoom { maths.panRoom(view, frame) }
    public var menuItems: [ZoomMenuItem] { maths.menu(view, frame, fitLabel: fitLabel) }
    var ready: Bool { frame.natural.width > 0 && frame.natural.height > 0 && frame.viewport.width > 0 && frame.viewport.height > 0 }

    // MARK: layout

    /// A picture (its natural px size, opening mode and largest zoom):
    /// a change re-opens at rest without animation.
    public func configure(natural: CGSize, initial: ZoomInitial, maxScale: Double?) {
        let next = ZoomFrame(natural: natural, viewport: frame.viewport, initial: initial, maxScale: maxScale)
        guard next != frame else { return }
        frame = next
        reset()
    }

    /// A new src: the rest view, no animation, no event.
    public func reset() {
        stopMomentum()
        gestureStart = nil
        pinching = false
        menuOpen = false
        rest = maths.rest(frame)
        view = rest.view
        reported = view.scale
        laidOut = ready
    }

    /// The viewport's size: the first one opens at rest; later ones re-fit
    /// at rest, else keep the scale about the centre.
    public func setViewport(_ size: CGSize) {
        guard size != frame.viewport else { return }
        let old = frame.viewport
        frame.viewport = size
        rest = maths.rest(frame)
        guard laidOut, old.width > 0, old.height > 0 else {
            reset()
            return
        }
        stopMomentum()
        view = maths.resize(view, from: old, frame)
        report()
    }

    // MARK: actions (the pill, keys, the bound scale)

    public func zoomIn() { settle(maths.step(view, direction: 1, frame)) }
    public func zoomOut() { settle(maths.step(view, direction: -1, frame)) }
    /// Back to the rest view.
    public func fit() { settle(rest.view) }

    /// A Menu row: Fit = the rest view, else the scale about the centre.
    public func select(_ item: ZoomMenuItem) {
        menuOpen = false
        if let s = item.scale { setScale(s) } else { fit() }
    }

    public func toggleMenu() {
        menuOpen.toggle()
        poke()
    }

    public func closeMenu() { menuOpen = false }

    /// The bound `scale` changed from outside: zoom to it about the centre.
    public func setScale(_ scale: Double) {
        guard scale.isFinite, scale > 0, abs(scale - view.scale) > 1e-4 else { return }
        settle(maths.zoomTo(view, scale: scale, anchor: centre, frame))
    }

    /// A hardware key (`+ = - − _ 0 1`, Escape closes the Menu); false = not ours.
    @discardableResult
    public func key(_ key: String) -> Bool {
        if key == "escape" || key == "\u{1b}" {
            guard menuOpen else { return false }
            menuOpen = false
            return true
        }
        guard ready, let next = maths.key(view, key: key, frame) else { return false }
        settle(next)
        return true
    }

    // MARK: gestures

    /// A pinch while held (`factor` since it began, about `anchor`).
    public func pinchChanged(factor: Double, anchor: CGPoint) {
        guard ready else { return }
        if !pinching {
            stopMomentum()
            pinching = true
            gestureStart = view
        }
        view = maths.pinch(gestureStart ?? view, factor: factor, anchor: anchor, frame)
        poke()
    }

    /// The pinch released: spring back inside the limits.
    public func pinchEnded() {
        guard pinching else { return }
        pinching = false
        gestureStart = nil
        settle(maths.clamp(view, frame))
    }

    /// Whether a one-finger drag pans now: only along an axis with room
    /// (with none, the swipe belongs to the host, a parent pager).
    public var canPan: Bool {
        let room = panRoom
        return ready && (room.x || room.y)
    }

    /// A drag while held (`translation` since it began).
    public func dragChanged(translation: CGSize) {
        guard ready, !pinching else { return }
        if gestureStart == nil {
            stopMomentum()
            gestureStart = view
            dragAxes = panRoom
            // A swipe along an axis without room is the host's (paging):
            // the picture stays put.
            let horizontal = abs(translation.width) > abs(translation.height)
            dragHandedOff = horizontal ? !dragAxes.x : !dragAxes.y
        }
        guard !dragHandedOff, let start = gestureStart else { return }
        view = maths.drag(start, dx: dragAxes.x ? translation.width : 0, dy: dragAxes.y ? translation.height : 0, frame)
    }

    /// The drag released at `velocity` (pt/s): past an edge it springs
    /// back, else the core's momentum carries it.
    public func dragEnded(velocity: CGSize) {
        guard gestureStart != nil, !pinching else { return }
        gestureStart = nil
        if dragHandedOff {
            dragHandedOff = false
            return
        }
        let clamped = maths.clamp(view, frame)
        if clamped != view {
            settle(clamped, report: false)
            return
        }
        let v = CGVector(dx: dragAxes.x ? velocity.width / 1000 : 0, dy: dragAxes.y ? velocity.height / 1000 : 0)
        guard maths.momentumActive(velocity: v, t: 0) else { return }
        startMomentum(from: view, velocity: v)
    }

    /// A double tap / click at `point`: at rest → in about it; else → rest.
    public func doubleTap(at point: CGPoint) {
        guard ready else { return }
        menuOpen = false
        settle(maths.doubleTap(view, at: point, frame))
    }

    /// A single tap on the picture: closes the Menu, else toggles the pill
    /// (`controls: auto`) and fires `tap`.
    public func tap() {
        if menuOpen {
            menuOpen = false
            return
        }
        if controls == .auto {
            pillShown.toggle()
            if pillShown { scheduleHide() } else { hideTask?.cancel() }
        }
        onTap?()
    }

    /// One wheel event (web-signed deltas; `zoom` = ⌘/ctrl held or a
    /// trackpad pinch): zooms about `anchor`, else pans while zoomed.
    public func wheel(deltaX: Double, deltaY: Double, lines: Bool, zoom: Bool, anchor: CGPoint) {
        guard ready else { return }
        stopMomentum()
        if zoom {
            view = maths.wheel(view, deltaY: deltaY, lines: lines, anchor: anchor, frame)
            poke()
            // Reported once the wheel rests.
            wheelTask?.cancel()
            wheelTask = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(150))
                guard !Task.isCancelled else { return }
                self?.report()
            }
        } else if canPan {
            // style.json `wheelLinePx`: a line-mode wheel pans 16 pt a line.
            let k = lines ? 16.0 : 1.0
            view = maths.scroll(view, deltaX: deltaX * k, deltaY: deltaY * k, frame)
        }
    }

    // MARK: the pill

    /// The pill's first appearance (`auto`: shown, then hidden when idle).
    public func appeared() {
        switch controls {
        case .always: pillShown = true
        case .none: pillShown = false
        case .auto:
            pillShown = true
            scheduleHide()
        }
    }

    /// A pointer move, a zoom, focus inside: show the pill and restart the idle timer.
    public func poke() {
        guard controls == .auto else { return }
        pillShown = true
        scheduleHide()
    }

    /// The pointer is over the pill (kept while hovered).
    public func pillHover(_ hovering: Bool) {
        pillHovered = hovering
        if hovering { hideTask?.cancel() } else { poke() }
    }

    private func scheduleHide() {
        hideTask?.cancel()
        guard controls == .auto else { return }
        let delay = metrics.autoHideMs
        hideTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(delay)))
            guard !Task.isCancelled, let self, !self.pillHovered, !self.menuOpen else { return }
            if self.reducedMotion {
                self.pillShown = false
            } else {
                withAnimation(self.motion) { self.pillShown = false }
            }
        }
    }

    // MARK: settling

    var centre: CGPoint { CGPoint(x: frame.viewport.width / 2, y: frame.viewport.height / 2) }

    /// Move to `next` (animated with `motion` unless reduced), then report.
    private func settle(_ next: ZoomView, report shouldReport: Bool = true) {
        stopMomentum()
        if next != view {
            if reducedMotion {
                view = next
            } else {
                withAnimation(motion) { view = next }
            }
        }
        poke()
        if shouldReport { report() }
    }

    /// The `zoom` event after a settled scale change.
    private func report() {
        guard reported.map({ abs($0 - view.scale) > 1e-6 }) ?? true else { return }
        reported = view.scale
        onZoom?(ZoomEvent(scale: view.scale, atFit: atRest))
    }

    private func startMomentum(from released: ZoomView, velocity: CGVector) {
        stopMomentum()
        let started = Date()
        momentumTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(16))
                guard !Task.isCancelled, let self else { return }
                let t = Date().timeIntervalSince(started) * 1000
                self.view = self.maths.momentum(released, velocity: velocity, t: t, self.frame)
                if !self.maths.momentumActive(velocity: velocity, t: t) { return }
            }
        }
    }

    private func stopMomentum() {
        momentumTask?.cancel()
        momentumTask = nil
    }
}
