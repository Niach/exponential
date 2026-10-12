import ExpCore
import SwiftUI

/// (4) THE MARK (EXP-1271, EXP-1259 D8.4): the logo disc whose three cut
/// stripes sweep in turn — vector, from the logo's own bezier data
/// (`FeedbackStates.Mark.stripes`, the one copy `ExpLogoMark` draws too), on
/// the shared timeline (`FeedbackStates.Mark.stripeSpan`, replayed against
/// `feedback-states.json` `mark.cases`). Each visible span of a stripe's
/// cubic is sampled at `samples` steps and cut out of the disc
/// (`destinationOut`), so the caller's ground shows through. A 1.2 s linear
/// loop entered at its rest (`phase(sinceStart:)`): the first frame is the
/// static logo, so a timeline that never advances still shows the logo, never
/// the loop's plain-disc phase 0. Reduce Motion, `animated: false`, a paused
/// or slowed timeline and the UI-test capture seam (`-uiTesting`, so a shot
/// never freezes a random frame) = the static logo, all three cuts whole.
///
/// Used for `starting` (the composer after Start, a run's status row before
/// its first turn) and `blocking` (`BlockingOverlay`).
public struct FeedbackMark: View {
    let size: CGFloat
    var color: Color
    var animated: Bool

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The mark's first frame: the loop is timed from here, not the clock.
    @State private var start = Date()

    /// Screenshot runs draw the static logo: an animating mark would freeze a
    /// different frame in every capture.
    static let capturing = ProcessInfo.processInfo.arguments.contains("-uiTesting")

    public init(size: CGFloat = CGFloat(FeedbackStates.Mark.inlineSize), color: Color = .white, animated: Bool = true) {
        self.size = size
        self.color = color
        self.animated = animated
    }

    public var body: some View {
        Group {
            if animated && !reduceMotion && !Self.capturing {
                TimelineView(.animation) { context in
                    MarkCanvas(color: color, spans: Self.spans(context, since: start))
                }
            } else {
                MarkCanvas(color: color, spans: FeedbackStates.Mark.staticSpans)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
        .accessibilityIdentifier(FeedbackStates.TestId.mark)
    }

    /// The live frame's spans; a paused or slowed timeline (low power, the
    /// Always-On display) shows the static logo instead of a stale frame.
    static func spans(_ context: TimelineViewDefaultContext, since start: Date) -> [ClosedRange<Double>?] {
        guard context.cadence == .live else { return FeedbackStates.Mark.staticSpans }
        return FeedbackStates.Mark.spans(
            phase: FeedbackStates.Mark.phase(sinceStart: context.date.timeIntervalSince(start))
        )
    }
}

/// The disc with the given stripe spans cut out of it.
struct MarkCanvas: View {
    let color: Color
    /// Per stripe, the visible part of its cut; nil = not drawn.
    let spans: [ClosedRange<Double>?]

    private static let grid: CGFloat = 100

    var body: some View {
        Canvas { context, canvasSize in
            let scale = min(canvasSize.width, canvasSize.height) / Self.grid
            let transform = CGAffineTransform(scaleX: scale, y: scale)
            context.fill(
                Path(ellipseIn: CGRect(x: 0, y: 0, width: Self.grid, height: Self.grid)).applying(transform),
                with: .color(color)
            )
            context.blendMode = .destinationOut
            for (index, span) in spans.enumerated() {
                guard let span, span.upperBound > span.lowerBound else { continue }
                context.stroke(
                    Self.path(index, span: span).applying(transform),
                    with: .color(.black),
                    style: StrokeStyle(
                        lineWidth: CGFloat(FeedbackStates.Mark.stripeWidth) * scale,
                        lineCap: .butt,
                        lineJoin: .round
                    )
                )
            }
        }
    }

    /// The stripe's cubic between `span`'s parameters, as a polyline of
    /// `samples` steps.
    static func path(_ index: Int, span: ClosedRange<Double>) -> Path {
        var path = Path()
        let steps = FeedbackStates.Mark.samples
        for step in 0...steps {
            let t = span.lowerBound + (span.upperBound - span.lowerBound) * Double(step) / Double(steps)
            let point = FeedbackStates.Mark.stripePoint(index, t: t)
            let cg = CGPoint(x: point.x, y: point.y)
            if step == 0 { path.move(to: cg) } else { path.addLine(to: cg) }
        }
        return path
    }
}

// MARK: - Blocking

/// (5) BLOCKING — the window ground at `backdropOpacity` over everything, the
/// mark at 40 and ONE muted line. ONLY switching team, switching account and
/// signing out; never for a single list or button. Hosted once at the app
/// root (`blockingOverlayHost`), driven by `BlockingFeedback`.
public struct BlockingOverlay: View {
    let line: String

    public init(_ line: String) {
        self.line = line
    }

    public var body: some View {
        ZStack {
            GlassTokens.backgroundBottom
                .opacity(FeedbackStates.Blocking.backdropOpacity)
                .ignoresSafeArea()
            VStack(spacing: CGFloat(FeedbackStates.Blocking.gap)) {
                FeedbackMark(size: CGFloat(FeedbackStates.Blocking.markSize))
                Text(line)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        // It blocks: nothing under it takes a tap while it is up.
        .contentShape(Rectangle())
        .onTapGesture {}
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(FeedbackStates.TestId.blocking)
    }
}

/// The app's ONE blocking state. `run(_:work:)` raises the overlay at once,
/// awaits the work and keeps it up for at least `minimumVisible` (a local
/// team switch is instant: without the floor the overlay would strobe).
@MainActor @Observable
public final class BlockingFeedback {
    public static let shared = BlockingFeedback()

    /// The line on screen, nil = no overlay.
    public private(set) var line: String?
    /// The floor that keeps a fast switch from flashing the overlay.
    public static let minimumVisible: Duration = .milliseconds(450)

    public init() {}

    public func run(_ line: String, work: @MainActor () async -> Void) async {
        self.line = line
        let clock = ContinuousClock()
        let started = clock.now
        await work()
        let elapsed = clock.now - started
        if elapsed < Self.minimumVisible {
            try? await Task.sleep(for: Self.minimumVisible - elapsed)
        }
        if self.line == line { self.line = nil }
    }

    /// Fire-and-forget form for a synchronous switch.
    public func show(_ line: String, during work: @escaping @MainActor () -> Void) {
        Task { await run(line) { work() } }
    }
}

private struct BlockingOverlayHost: ViewModifier {
    let feedback: BlockingFeedback
    @Environment(\.motion) private var motion

    func body(content: Content) -> some View {
        content.overlay {
            if let line = feedback.line {
                BlockingOverlay(line)
                    .transition(.opacity)
            }
        }
        .animation(motion.fast, value: feedback.line)
    }
}

public extension View {
    /// Mounts the blocking overlay over this whole view (the app root).
    func blockingOverlayHost(_ feedback: BlockingFeedback = .shared) -> some View {
        modifier(BlockingOverlayHost(feedback: feedback))
    }
}
