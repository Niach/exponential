import SwiftUI
import ExponentialUICore

/// A node's keyframe animation (round-2 contract §2): the name and the
/// timing the core resolved against the theme (`FfiVisual.animationJson`).
/// The painter never re-derives keyframes: it samples the core's
/// `animationFrameJson` per display frame.
public struct KeyframeAnimation: Equatable, Sendable {
    public let name: String
    /// `{durationMs, easing, iterations}` as the core wrote it.
    public let timingJson: String
    public let durationMs: Double
    /// nil = infinite.
    public let iterations: Double?
    /// Round 4 motion: the stagger offset + `animationDelay` (ms): the first
    /// keyframe holds meanwhile (the core's frame does it).
    public let delayMs: Double

    init?(json: String) {
        guard let o = JSONValue.parse(json).object, let name = o["name"]?.string, let timing = o["timing"] else { return nil }
        self.name = name
        timingJson = timing.json
        if case let .number(d)? = timing["durationMs"] { durationMs = d } else { durationMs = 0 }
        if case let .number(n)? = timing["iterations"] { iterations = n } else { iterations = nil }
        if case let .number(d)? = timing["delayMs"] { delayMs = d } else { delayMs = 0 }
    }

    /// The whole run (nil = forever), the delay included.
    var totalMs: Double? { iterations.map { delayMs + $0 * durationMs } }

    /// The frame `elapsedMs` after the node entered the tree (reduced motion = the rest frame).
    func frame(at elapsedMs: Double, reducedMotion: Bool) -> AnimationFrame {
        let json = (try? animationFrameJson(name: name, timingJson: timingJson, elapsedMs: elapsedMs, reducedMotion: reducedMotion)) ?? nil
        return AnimationFrame(json.map(JSONValue.parse) ?? .object([:]))
    }
}

/// One sampled frame: physical channels composed OUTSIDE the node's own
/// transform about the box centre; opacity MULTIPLIES the node's own.
public struct AnimationFrame: Equatable, Sendable {
    public var opacity: Double = 1
    public var translateX: CGFloat = 0
    public var translateY: CGFloat = 0
    public var rotate: Double = 0
    public var scale: CGFloat = 1
    /// The shimmer band's position in box widths (−1 … 1); nil = no band.
    public var band: CGFloat?

    init(_ v: JSONValue) {
        func n(_ k: String) -> Double? { if case let .number(x)? = v[k] { return x } else { return nil } }
        opacity = n("opacity") ?? 1
        translateX = CGFloat(n("translateX") ?? 0)
        translateY = CGFloat(n("translateY") ?? 0)
        rotate = n("rotate") ?? 0
        scale = CGFloat(n("scale") ?? 1)
        band = n("band").map { CGFloat($0) }
    }

    /// The painted opacity: the node's own × the frame's (`paintedOpacity`).
    public func paintedOpacity(own: Double) -> Double { own * min(1, max(0, opacity)) }
}

/// Paints a node's keyframe animation from the core's frames with a
/// `TimelineView`; a finished run keeps its end frame, reduced motion the
/// rest frame (no timeline at all).
struct KeyframeAnimationModifier: ViewModifier {
    let animation: KeyframeAnimation?
    let reducedMotion: Bool
    let size: CGSize
    let bandColor: Color?
    @State private var start = Date()

    func body(content: Content) -> some View {
        if let animation {
            if reducedMotion {
                apply(content, animation.frame(at: 0, reducedMotion: true))
            } else {
                TimelineView(.animation(paused: false)) { context in
                    let elapsed = context.date.timeIntervalSince(start) * 1000
                    let at = animation.totalMs.map { min(elapsed, $0) } ?? elapsed
                    apply(content, animation.frame(at: at, reducedMotion: false))
                }
                .onAppear { start = Date() }
            }
        } else {
            content
        }
    }

    private func apply(_ content: Content, _ f: AnimationFrame) -> some View {
        content
            .overlay {
                if let band = f.band, let bandColor {
                    // The shimmer: a band of the background colour (alpha
                    // 0 → .5 → 0) sweeping −1 → 1 box widths, clipped.
                    LinearGradient(colors: [bandColor.opacity(0), bandColor.opacity(0.5), bandColor.opacity(0)], startPoint: .leading, endPoint: .trailing)
                        .frame(width: size.width, height: size.height)
                        .offset(x: band * size.width)
                        .clipped()
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
            }
            .opacity(min(1, max(0, f.opacity)))
            .scaleEffect(f.scale, anchor: .center)
            .rotationEffect(.degrees(f.rotate), anchor: .center)
            .offset(x: f.translateX, y: f.translateY)
    }
}

/// Round 4 motion: a leaving ghost plays its exit from the core's frames
/// (from the moment it began leaving), takes no input and leaves the
/// accessibility tree; once the exit ran the model finishes it (the ghost
/// leaves, layout closes its gap).
struct ExitAnimationModifier: ViewModifier {
    let id: String
    let exit: KeyframeAnimation?
    let exiting: Bool
    let model: SurfaceModel
    @State private var start = Date()

    func body(content: Content) -> some View {
        if exiting, let exit {
            TimelineView(.animation(paused: false)) { context in
                let elapsed = min(context.date.timeIntervalSince(start) * 1000, exit.totalMs ?? 0)
                let f = exit.frame(at: elapsed, reducedMotion: false)
                content
                    .opacity(min(1, max(0, f.opacity)))
                    .scaleEffect(f.scale, anchor: .center)
                    .rotationEffect(.degrees(f.rotate), anchor: .center)
                    .offset(x: f.translateX, y: f.translateY)
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            .onAppear { start = Date() }
            .task(id: id) {
                try? await Task.sleep(nanoseconds: UInt64(max(0, exit.totalMs ?? 0) * 1_000_000))
                model.finishExit(id)
            }
        } else if exiting {
            content.allowsHitTesting(false).accessibilityHidden(true)
        } else {
            content
        }
    }
}

/// `backdropBlur` (round-2 §2): the platform material behind a translucent
/// background, by radius (sm / md thin, lg / xl regular). Paints under the
/// box's own background, inside its shape.
struct BackdropBlurModifier: ViewModifier {
    let style: PaintStyle
    let size: CGSize

    func body(content: Content) -> some View {
        if let r = style.backdropBlur {
            content.background {
                if r <= 12 {
                    Rectangle().fill(.ultraThinMaterial).clipShape(style.shape(size))
                } else {
                    Rectangle().fill(.regularMaterial).clipShape(style.shape(size))
                }
            }
        } else {
            content
        }
    }
}

/// A `position: sticky` node or a pinned section header (round 2): painted
/// moved by the core's offset, above its siblings.
struct StickyModifier: ViewModifier {
    let offset: CGSize?

    func body(content: Content) -> some View {
        if let offset {
            content.offset(offset).zIndex(1)
        } else {
            content
        }
    }
}
