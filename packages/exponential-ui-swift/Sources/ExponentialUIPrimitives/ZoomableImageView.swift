import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// EXP-1586: a picture that zooms and pans (the catalog `ZoomableImage`;
/// the Exponential app's lightbox). The view fills the size its parent
/// proposes (the viewport) and draws `content` laid out at the picture's
/// NATURAL size (px; 1 = 100% = one image pixel per point), scaled and
/// offset by the current view. Every number comes from the core through
/// `maths` (`ZoomMaths.core` in `ExponentialUI`).
///
/// Input:
/// - pinch (touch, trackpad) zooms about the fingers, rubber-banding past
///   the limits while held and springing back on release (`motion`;
///   reduced motion snaps);
/// - a one-finger / mouse drag pans, but ONLY while an axis has pan room:
///   at rest (nothing to pan) no drag is installed, so a swipe reaches the
///   host (a parent pager); a swipe along an axis without room is left alone;
///   the release carries the core's momentum and springs back past an edge;
/// - double tap / click: rest → max(100%, 2 × rest) about the point,
///   zoomed → rest; a single tap closes the Menu, else toggles the pill
///   (`controls: auto`) and calls `onTap`;
/// - pointer (Mac, iPad): ⌘/ctrl + scroll zooms about the pointer, a plain
///   scroll pans while zoomed; a pointer move shows the pill;
/// - keys (`keyboard: true`, or `command` from the host's focus routing):
///   `+ =` step in, `-` step out, `0` the rest view, `1` 100%.
///
/// THE PILL (`controls` auto / always / none) sits bottom-centre
/// `metrics.inset` above the picture's bottom edge; `pill` draws it
/// (default `ZoomPill`). A new `resetID` (a new src) or natural size opens
/// at rest without animation; a resize re-fits at rest, else keeps the
/// scale about the centre. `onZoom` fires after each settled scale change;
/// `scale` (optional) is written then and followed when it changes outside.
///
/// Large pictures: pass any content sized to the natural size, e.g. a
/// stack of horizontal strips that keeps every layer under Core
/// Animation's texture cap (a 25k px full-page capture).
///
/// ```swift
/// ZoomableImageView(image: Image(uiImage: shot), naturalSize: CGSize(width: 1440, height: 25094), maths: .core,
///                   scale: $scale, onTap: { chromeHidden.toggle() })
///
/// ZoomableImageView(naturalSize: size, maths: .core, controls: .auto, resetID: attachmentId) {
///     StripImageView(strips: strips, width: size.width)       // laid out at the natural size
/// }
/// ```
public struct ZoomableImageView<Content: View, Pill: View>: View {
    let naturalSize: CGSize
    let initial: ZoomInitial
    let maxScale: Double?
    let controls: ZoomControls
    let scale: Binding<Double>?
    let resetID: AnyHashable?
    let reducedMotion: Bool
    let motion: Animation?
    let label: String
    let keyboard: Bool
    let command: ZoomCommand?
    let cornerRadius: CGFloat
    let metrics: ZoomPillMetrics
    let fitLabel: String
    let onZoom: ((ZoomEvent) -> Void)?
    let onTap: (() -> Void)?
    let content: Content
    let pill: (ZoomController) -> Pill

    @State private var controller: ZoomController
    @Environment(\.accessibilityReduceMotion) private var platformReduced

    /// The full form: any `content` (laid out at `naturalSize`) and a custom pill.
    /// - Parameters:
    ///   - naturalSize: the picture's size in px (its 100% size in pt).
    ///   - maths: the core's zoom entry (`ZoomMaths.core`).
    ///   - initial: the opening zoom (`auto` = fitWidth for a tall picture, else fit).
    ///   - maxScale: the largest zoom (nil = 8, never below fit).
    ///   - controls: the pill: `auto` (shown, hidden after 2 s idle; a tap toggles it), `always`, `none`.
    ///   - scale: written after each settled zoom change; followed (about the centre) when set outside.
    ///   - resetID: a new value (a new src) opens at rest without animation.
    ///   - reducedMotion: also snap (the platform's Reduce Motion always does).
    ///   - motion: the spring-back / step animation (nil = 200 ms ease-out).
    ///   - label: the picture's accessible name (the alt).
    ///   - keyboard: take focus and read `+ = - 0 1` itself.
    ///   - command: a key the host routed here (applied once per serial).
    ///   - cornerRadius: the picture's clip radius (the pill is never clipped).
    ///   - fitLabel: the Menu's Fit row.
    ///   - onZoom: `{scale, atFit}` after each settled zoom change.
    ///   - onTap: a single tap on the picture.
    public init(
        naturalSize: CGSize,
        maths: ZoomMaths,
        initial: ZoomInitial = .auto,
        maxScale: Double? = nil,
        controls: ZoomControls = .auto,
        scale: Binding<Double>? = nil,
        resetID: AnyHashable? = nil,
        reducedMotion: Bool = false,
        motion: Animation? = nil,
        label: String = "",
        keyboard: Bool = false,
        command: ZoomCommand? = nil,
        cornerRadius: CGFloat = 0,
        metrics: ZoomPillMetrics = .standard,
        fitLabel: String = ZoomStrings.english.zoomFit,
        onZoom: ((ZoomEvent) -> Void)? = nil,
        onTap: (() -> Void)? = nil,
        @ViewBuilder content: () -> Content,
        @ViewBuilder pill: @escaping (ZoomController) -> Pill
    ) {
        self.naturalSize = naturalSize
        self.initial = initial
        self.maxScale = maxScale
        self.controls = controls
        self.scale = scale
        self.resetID = resetID
        self.reducedMotion = reducedMotion
        self.motion = motion
        self.label = label
        self.keyboard = keyboard
        self.command = command
        self.cornerRadius = cornerRadius
        self.metrics = metrics
        self.fitLabel = fitLabel
        self.onZoom = onZoom
        self.onTap = onTap
        self.content = content()
        self.pill = pill
        _controller = State(initialValue: ZoomController(maths: maths))
    }

    public var body: some View {
        let c = controller
        let view = c.view
        Color.clear
            .overlay(alignment: .topLeading) {
                content
                    .frame(width: naturalSize.width, height: naturalSize.height)
                    .scaleEffect(view.scale, anchor: .topLeading)
                    .offset(x: view.x, y: view.y)
                    .allowsHitTesting(false)
            }
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
            .contentShape(Rectangle())
            .modifier(ZoomGestures(controller: c))
            .modifier(ZoomPointer(controller: c))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text(label))
            .accessibilityValue(Text(c.percent))
            .accessibilityAddTraits(.isImage)
            .accessibilityAdjustableAction { direction in
                if direction == .increment { c.zoomIn() } else { c.zoomOut() }
            }
            .overlay(alignment: .bottom) {
                if c.pillShown, controls != .none {
                    pill(c)
                        .padding(.bottom, metrics.inset)
                        .transition(.opacity)
                }
            }
            .modifier(ZoomKeyboard(controller: c, on: keyboard))
            .onGeometryChange(for: CGSize.self) { $0.size } action: { c.setViewport($0) }
            .onAppear {
                sync(c)
                c.configure(natural: naturalSize, initial: initial, maxScale: maxScale)
                c.appeared()
            }
            .onChange(of: naturalSize) { _, _ in c.configure(natural: naturalSize, initial: initial, maxScale: maxScale) }
            .onChange(of: initial) { _, _ in c.configure(natural: naturalSize, initial: initial, maxScale: maxScale) }
            .onChange(of: maxScale) { _, _ in c.configure(natural: naturalSize, initial: initial, maxScale: maxScale) }
            .onChange(of: resetID) { _, _ in c.reset() }
            .onChange(of: controls) { _, _ in
                sync(c)
                c.appeared()
            }
            .onChange(of: reducedMotion || platformReduced) { _, _ in sync(c) }
            .onChange(of: scale?.wrappedValue) { _, next in
                if let next { c.setScale(next) }
            }
            .onChange(of: command) { _, next in
                if let next { c.key(next.key) }
            }
    }

    private func sync(_ c: ZoomController) {
        c.controls = controls
        c.reducedMotion = reducedMotion || platformReduced
        c.motion = motion ?? .timingCurve(0.2, 0, 0, 1, duration: 0.2)
        c.metrics = metrics
        c.fitLabel = fitLabel
        let scale = scale
        let onZoom = onZoom
        c.onZoom = { event in
            scale?.wrappedValue = event.scale
            onZoom?(event)
        }
        c.onTap = onTap
    }
}

public extension ZoomableImageView where Pill == ZoomPill {
    /// Any `content` laid out at `naturalSize`, with THE default pill.
    init(
        naturalSize: CGSize,
        maths: ZoomMaths,
        initial: ZoomInitial = .auto,
        maxScale: Double? = nil,
        controls: ZoomControls = .auto,
        scale: Binding<Double>? = nil,
        resetID: AnyHashable? = nil,
        reducedMotion: Bool = false,
        label: String = "",
        keyboard: Bool = true,
        pillStyle: ZoomPillStyle = .standard(),
        strings: ZoomStrings = .english,
        onZoom: ((ZoomEvent) -> Void)? = nil,
        onTap: (() -> Void)? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.init(naturalSize: naturalSize, maths: maths, initial: initial, maxScale: maxScale, controls: controls, scale: scale, resetID: resetID, reducedMotion: reducedMotion, label: label, keyboard: keyboard, metrics: pillStyle.metrics, fitLabel: strings.zoomFit, onZoom: onZoom, onTap: onTap, content: content) { c in
            ZoomPill(controller: c, style: pillStyle, strings: strings)
        }
    }
}

public extension ZoomableImageView where Content == ZoomImageContent, Pill == ZoomPill {
    /// A SwiftUI `Image` of `naturalSize` px, with THE default pill.
    init(
        image: Image,
        naturalSize: CGSize,
        maths: ZoomMaths,
        initial: ZoomInitial = .auto,
        maxScale: Double? = nil,
        controls: ZoomControls = .auto,
        scale: Binding<Double>? = nil,
        resetID: AnyHashable? = nil,
        label: String = "",
        keyboard: Bool = true,
        pillStyle: ZoomPillStyle = .standard(),
        strings: ZoomStrings = .english,
        onZoom: ((ZoomEvent) -> Void)? = nil,
        onTap: (() -> Void)? = nil
    ) {
        self.init(naturalSize: naturalSize, maths: maths, initial: initial, maxScale: maxScale, controls: controls, scale: scale, resetID: resetID, label: label, keyboard: keyboard, pillStyle: pillStyle, strings: strings, onZoom: onZoom, onTap: onTap) {
            ZoomImageContent(image: image)
        }
    }

    #if canImport(UIKit)
    /// A `UIImage` at its pixel size (`size × scale`), with THE default pill.
    init(
        uiImage: UIImage,
        maths: ZoomMaths,
        initial: ZoomInitial = .auto,
        maxScale: Double? = nil,
        controls: ZoomControls = .auto,
        scale: Binding<Double>? = nil,
        label: String = "",
        pillStyle: ZoomPillStyle = .standard(),
        strings: ZoomStrings = .english,
        onZoom: ((ZoomEvent) -> Void)? = nil,
        onTap: (() -> Void)? = nil
    ) {
        let px = CGSize(width: uiImage.size.width * uiImage.scale, height: uiImage.size.height * uiImage.scale)
        self.init(image: Image(uiImage: uiImage), naturalSize: px, maths: maths, initial: initial, maxScale: maxScale, controls: controls, scale: scale, resetID: ObjectIdentifier(uiImage), label: label, pillStyle: pillStyle, strings: strings, onZoom: onZoom, onTap: onTap)
    }
    #endif
}

public extension ZoomableImageView where Content == ZoomAsyncImageContent, Pill == ZoomPill {
    /// A picture `load` fetches (a spinner until then, nothing on nil);
    /// `naturalSize` must be known up front (the stored attachment's size).
    init(
        naturalSize: CGSize,
        maths: ZoomMaths,
        initial: ZoomInitial = .auto,
        maxScale: Double? = nil,
        controls: ZoomControls = .auto,
        scale: Binding<Double>? = nil,
        resetID: AnyHashable? = nil,
        label: String = "",
        pillStyle: ZoomPillStyle = .standard(),
        strings: ZoomStrings = .english,
        onZoom: ((ZoomEvent) -> Void)? = nil,
        onTap: (() -> Void)? = nil,
        load: @escaping @Sendable () async -> Image?
    ) {
        self.init(naturalSize: naturalSize, maths: maths, initial: initial, maxScale: maxScale, controls: controls, scale: scale, resetID: resetID, label: label, pillStyle: pillStyle, strings: strings, onZoom: onZoom, onTap: onTap) {
            ZoomAsyncImageContent(id: resetID, load: load)
        }
    }
}

/// An `Image` stretched over the natural-size box.
public struct ZoomImageContent: View {
    let image: Image

    public init(image: Image) { self.image = image }

    public var body: some View {
        image.resizable().interpolation(.high)
    }
}

/// A loaded `Image` over the natural-size box (a spinner while loading).
public struct ZoomAsyncImageContent: View {
    let id: AnyHashable?
    let load: @Sendable () async -> Image?
    @State private var image: Image?
    @State private var done = false

    public var body: some View {
        ZStack {
            if let image {
                image.resizable().interpolation(.high)
            } else if !done {
                ProgressView()
            }
        }
        .task(id: id) {
            done = false
            image = await load()
            done = true
        }
    }
}

// MARK: - input

/// Pinch, drag (only with pan room), double + single tap.
struct ZoomGestures: ViewModifier {
    let controller: ZoomController

    func body(content: Content) -> some View {
        let c = controller
        let pinch = MagnifyGesture()
            .onChanged { v in c.pinchChanged(factor: Double(v.magnification), anchor: v.startLocation) }
            .onEnded { _ in c.pinchEnded() }
        let drag = DragGesture(minimumDistance: 6)
            .onChanged { v in c.dragChanged(translation: v.translation) }
            .onEnded { v in c.dragEnded(velocity: v.velocity) }
        let taps = SpatialTapGesture(count: 2)
            .onEnded { v in c.doubleTap(at: v.location) }
            .exclusively(before: SpatialTapGesture(count: 1).onEnded { _ in c.tap() })
        content
            .gesture(taps)
            .simultaneousGesture(pinch)
            // No pan room = no drag: the swipe belongs to the host.
            .simultaneousGesture(drag, including: c.canPan ? .all : .subviews)
    }
}

/// `+ = - 0 1` (and Escape for the Menu) when the view takes focus itself.
struct ZoomKeyboard: ViewModifier {
    let controller: ZoomController
    let on: Bool

    func body(content: Content) -> some View {
        if on {
            content
                .focusable()
                .focusEffectDisabled()
                .onKeyPress(phases: [.down, .repeat]) { press in
                    if press.key == .escape { return controller.key("escape") ? .handled : .ignored }
                    return controller.key(press.characters) ? .handled : .ignored
                }
        } else {
            content
        }
    }
}

/// The pointer: hover shows the pill; scroll-wheel events zoom (⌘/ctrl) or
/// pan. A transparent platform view that takes ONLY scroll events (every
/// touch and click passes through it).
struct ZoomPointer: ViewModifier {
    let controller: ZoomController

    func body(content: Content) -> some View {
        content
            .onContinuousHover { phase in
                if case .active = phase { controller.poke() }
            }
            .overlay { ZoomScrollCatcher(controller: controller) }
    }
}

#if canImport(UIKit)
struct ZoomScrollCatcher: UIViewRepresentable {
    let controller: ZoomController

    func makeUIView(context: Context) -> CatcherView {
        let v = CatcherView()
        let pan = UIPanGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.scrolled(_:)))
        pan.allowedScrollTypesMask = .all
        pan.allowedTouchTypes = []
        v.addGestureRecognizer(pan)
        return v
    }

    func updateUIView(_ uiView: CatcherView, context: Context) {
        context.coordinator.controller = controller
    }

    func makeCoordinator() -> Coordinator { Coordinator(controller: controller) }

    final class CatcherView: UIView {
        override init(frame: CGRect) {
            super.init(frame: frame)
            backgroundColor = .clear
        }

        required init?(coder: NSCoder) { fatalError("init(coder:)") }

        /// Scroll events only: touches reach SwiftUI's gestures below.
        override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
            event?.type == .scroll ? super.hitTest(point, with: event) : nil
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        var controller: ZoomController
        private var last = CGPoint.zero

        init(controller: ZoomController) { self.controller = controller }

        @objc func scrolled(_ pan: UIPanGestureRecognizer) {
            let t = pan.translation(in: pan.view)
            if pan.state == .began { last = .zero }
            let d = CGPoint(x: t.x - last.x, y: t.y - last.y)
            last = t
            let zoom = pan.modifierFlags.contains(.command) || pan.modifierFlags.contains(.control)
            // The pan moves the content with the fingers: the web's wheel
            // delta is the opposite.
            controller.wheel(deltaX: -Double(d.x), deltaY: -Double(d.y), lines: false, zoom: zoom, anchor: pan.location(in: pan.view))
        }
    }
}
#elseif canImport(AppKit)
struct ZoomScrollCatcher: NSViewRepresentable {
    let controller: ZoomController

    func makeNSView(context: Context) -> CatcherView {
        let v = CatcherView()
        v.controller = controller
        return v
    }

    func updateNSView(_ nsView: CatcherView, context: Context) {
        nsView.controller = controller
    }

    final class CatcherView: NSView {
        var controller: ZoomController?

        override var isFlipped: Bool { true }

        /// Scroll-wheel events only: clicks and drags reach SwiftUI below.
        override func hitTest(_ point: NSPoint) -> NSView? {
            NSApp?.currentEvent?.type == .scrollWheel ? super.hitTest(point) : nil
        }

        override func scrollWheel(with event: NSEvent) {
            guard let controller else { return super.scrollWheel(with: event) }
            let p = convert(event.locationInWindow, from: nil)
            let zoom = event.modifierFlags.contains(.command) || event.modifierFlags.contains(.control)
            let lines = !event.hasPreciseScrollingDeltas
            if !zoom, !controller.canPan { return super.scrollWheel(with: event) }
            // AppKit's deltas move the content; the web's wheel delta is the opposite.
            MainActor.assumeIsolated {
                controller.wheel(deltaX: -Double(event.scrollingDeltaX), deltaY: -Double(event.scrollingDeltaY), lines: lines, zoom: zoom, anchor: p)
            }
        }
    }
}
#endif
