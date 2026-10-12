import ExpCore
import SwiftUI

// EXP-1275 (EXP-1259 D8): THE feedback states on iOS, the low-level three —
// the inline spinner, the pending button state, the skeleton rows — plus the
// mark and the blocking overlay. Constants are `FeedbackStates`
// (feedback-states.json ×4); web `@exp/ui` `Spinner`/`Skeleton`/Button
// `pending`, desktop `ui::feedback`, Android `components/Feedback*.kt`.

/// (1) INLINE — the ONE small spinner: a 3/4 ring turning once per 0.8 s, in
/// the caller's muted foreground. The list-head spinner (`GlassSectionHeader`
/// / `GlassSectionBand` `loading`, EXP-1274) IS this primitive, and so is a
/// pending button's icon slot. Reduce Motion = a still ring.
public struct InlineSpinner: View {
    let size: CGFloat
    var color: Color

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(
        size: CGFloat = CGFloat(FeedbackStates.Inline.spinner),
        color: Color = .white.opacity(TextOpacity.tertiary)
    ) {
        self.size = size
        self.color = color
    }

    /// One turn, in seconds.
    static let period: TimeInterval = 0.8

    public var body: some View {
        Group {
            if reduceMotion {
                ring(angle: .zero)
            } else {
                TimelineView(.animation) { context in
                    let turn = context.date.timeIntervalSinceReferenceDate
                        .truncatingRemainder(dividingBy: Self.period) / Self.period
                    ring(angle: .degrees(turn * 360))
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityLabel(FeedbackStates.Inline.loading)
        .accessibilityIdentifier(FeedbackStates.TestId.spinner)
    }

    private func ring(angle: Angle) -> some View {
        Circle()
            .trim(from: 0, to: 0.75)
            .stroke(color, style: StrokeStyle(lineWidth: max(1.25, size / 8), lineCap: .round))
            .rotationEffect(angle)
    }
}

/// INLINE with its word: the spinner + a progressive word ("Saving") in a
/// band, header or caption; a settled word ("Saved") draws without the
/// spinner.
public struct InlineStatus: View {
    let word: String
    let busy: Bool

    public init(_ word: String, busy: Bool = true) {
        self.word = word
        self.busy = busy
    }

    public var body: some View {
        HStack(spacing: CGFloat(FeedbackStates.Inline.gap)) {
            if busy { InlineSpinner() }
            Text(word)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Pending

/// (2) PENDING's clock: true only once `active` has held for
/// `FeedbackStates.Pending.delayMs`, so a fast action never flashes its
/// spinner. Read with `.pendingDelay(active, shown: $shown)`.
private struct PendingDelayModifier: ViewModifier {
    let active: Bool
    @Binding var shown: Bool

    func body(content: Content) -> some View {
        content.task(id: active) {
            guard active else {
                shown = false
                return
            }
            try? await Task.sleep(for: .milliseconds(FeedbackStates.Pending.delayMs))
            if !Task.isCancelled { shown = true }
        }
    }
}

public extension View {
    /// Flips `shown` true `FeedbackStates.Pending.delayMs` after `active`
    /// turned true, and false the moment it turns false.
    func pendingDelay(_ active: Bool, shown: Binding<Bool>) -> some View {
        modifier(PendingDelayModifier(active: active, shown: shown))
    }
}

/// (2) PENDING — a button's label while what it started runs: the icon slot
/// becomes the spinner and the label the progressive word ("Starting"),
/// after the 150 ms delay. The idle label keeps the width (the pending face
/// draws over it), so the button never jumps.
public struct PendingLabel<Icon: View>: View {
    let label: String
    let pendingWord: String?
    let pending: Bool
    var spinnerColor: Color
    let icon: Icon

    @State private var shown = false

    public init(
        _ label: String,
        pendingWord: String? = nil,
        pending: Bool,
        spinnerColor: Color = .white.opacity(TextOpacity.secondary),
        @ViewBuilder icon: () -> Icon
    ) {
        self.label = label
        self.pendingWord = pendingWord
        self.pending = pending
        self.spinnerColor = spinnerColor
        self.icon = icon()
    }

    public var body: some View {
        // The idle face sizes the slot; the pending face overlays it, so the
        // width is the idle width at minimum.
        idle
            .opacity(shown ? 0 : 1)
            .overlay {
                if shown {
                    HStack(spacing: 8) {
                        InlineSpinner(size: CGFloat(FeedbackStates.Pending.spinner), color: spinnerColor)
                        Text(pendingWord ?? label)
                            .lineLimit(1)
                            .fixedSize()
                    }
                    .accessibilityIdentifier(FeedbackStates.TestId.pending)
                }
            }
            .pendingDelay(pending, shown: $shown)
    }

    private var idle: some View {
        HStack(spacing: 8) {
            icon
            Text(label).lineLimit(1)
        }
    }
}

extension PendingLabel where Icon == EmptyView {
    public init(
        _ label: String,
        pendingWord: String? = nil,
        pending: Bool,
        spinnerColor: Color = .white.opacity(TextOpacity.secondary)
    ) {
        self.init(label, pendingWord: pendingWord, pending: pending, spinnerColor: spinnerColor) { EmptyView() }
    }
}

// MARK: - Placeholder

/// (3) PLACEHOLDER — skeleton rows in the real row geometry: the row's own
/// height, a dot where its glyph sits, bars where its text sits, one slow
/// opacity pulse (`FeedbackStates.Placeholder`), static under Reduce Motion.
/// It replaces every "Loading…" line and the phones' centred first-load
/// spinners.
public struct SkeletonRows: View {
    let rows: Int
    let rowHeight: CGFloat
    /// Leading dot (a glyph slot) on every row.
    let dot: Bool
    /// Hairline rules between the rows (a flat list) or none (a picker).
    let dividers: Bool

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var bright = false

    public init(
        rows: Int = FeedbackStates.Placeholder.rows,
        rowHeight: CGFloat = 44,
        dot: Bool = true,
        dividers: Bool = true
    ) {
        self.rows = rows
        self.rowHeight = rowHeight
        self.dot = dot
        self.dividers = dividers
    }

    /// The second bar's widths, so the rows read as text, not a grid.
    private static let titleFractions: [CGFloat] = [0.62, 0.44, 0.7, 0.52, 0.58]

    private var opacity: Double {
        if reduceMotion { return FeedbackStates.Placeholder.reducedOpacity }
        return bright ? FeedbackStates.Placeholder.opacity.high : FeedbackStates.Placeholder.opacity.low
    }

    public var body: some View {
        VStack(spacing: 0) {
            ForEach(0..<rows, id: \.self) { index in
                row(index)
                if dividers, index < rows - 1 { GlassDivider() }
            }
        }
        .opacity(opacity)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(
                .easeInOut(duration: Double(FeedbackStates.Placeholder.pulseMs) / 2000)
                    .repeatForever(autoreverses: true)
            ) { bright = true }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(FeedbackStates.Inline.loading)
        .accessibilityIdentifier(FeedbackStates.TestId.skeleton)
    }

    private func row(_ index: Int) -> some View {
        GeometryReader { proxy in
            HStack(spacing: 10) {
                if dot {
                    Circle()
                        .fill(GlassTokens.fillActive)
                        .frame(
                            width: CGFloat(FeedbackStates.Placeholder.dot),
                            height: CGFloat(FeedbackStates.Placeholder.dot)
                        )
                }
                bar(width: min(64, proxy.size.width * 0.2))
                bar(width: proxy.size.width * Self.titleFractions[index % Self.titleFractions.count] * 0.75)
                Spacer(minLength: 0)
            }
            .frame(maxHeight: .infinity)
            .padding(.horizontal, 12)
        }
        .frame(height: rowHeight)
    }

    private func bar(width: CGFloat) -> some View {
        RoundedRectangle(cornerRadius: CGFloat(FeedbackStates.Placeholder.radius))
            .fill(GlassTokens.fillActive)
            .frame(width: max(0, width), height: CGFloat(FeedbackStates.Placeholder.barHeight))
    }
}
