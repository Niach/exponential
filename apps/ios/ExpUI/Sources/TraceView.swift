import ExpCore
import SwiftUI

// EXP-1324: THE Trace on iOS — shadcn's Trace (radix-nova) rebuilt from
// `packages/domain-contract/fixtures/trace.json` geometry (pt = px), the same
// look as web `@exp/ui` `Trace*`, desktop gpui and Compose. A LINE = [icon]
// [gap][title][gap][description] in ONE row of `lineHeight`; a COLLAPSIBLE
// appends the chevron and discloses its content over a left rail. Sizes:
// `default` (styleguide) and `sm` (the session transcript + issue activity).
// The pure models live in ExpCore `Trace` / `TaskQueue`.

/// The two Trace sizes (fixture `geometry.default` / `geometry.sm`).
public enum TraceSize: Sendable {
    case `default`
    case sm

    public var geometry: Trace.LineGeometry {
        switch self {
        case .default: Trace.Geometry.default
        case .sm: Trace.Geometry.sm
        }
    }
}

/// A Trace concept icon (`trace-terminal`, …) → its generated glyph name.
/// Concepts this build does not know draw the generic tool glyph.
public enum TraceIcons {
    public static func glyph(_ concept: String) -> String {
        switch concept {
        case "trace-background": AppIcons.traceBackground
        case "trace-chevron": AppIcons.traceChevron
        case "trace-completed": AppIcons.traceCompleted
        case "trace-edit": AppIcons.traceEdit
        case "trace-failed": AppIcons.traceFailed
        case "trace-fetch": AppIcons.traceFetch
        case "trace-read": AppIcons.traceRead
        case "trace-reasoning": AppIcons.traceReasoning
        case "trace-search": AppIcons.traceSearch
        case "trace-subagent": AppIcons.traceSubagent
        case "trace-task-done": AppIcons.traceTaskDone
        case "trace-task-pending": AppIcons.traceTaskPending
        case "trace-task-running": AppIcons.traceTaskRunning
        case "trace-terminal": AppIcons.traceTerminal
        case "trace-todo": AppIcons.traceTodo
        default: AppIcons.traceTool
        }
    }
}

// MARK: - Shimmer

/// The `motion.shimmer` token: a running Trace title painted as a text-clipped
/// linear gradient at 90° + `angleDeg`, 200% of the text width + 2 spread
/// wide, sliding so the highlight sweeps left → right over `durationMs`,
/// forever. Reduce motion = the plain text.
struct TraceShimmer: ViewModifier {
    let active: Bool
    /// The text colour as a neutral grey level (sRGB 0…1) — every Trace
    /// title colour is a neutral, which keeps the OKLCH highlight math exact.
    let baseWhite: Double

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private typealias Tokens = DesignTokens.Motion.Shimmer

    /// sRGB grey ↔ OKLab lightness for a neutral (chroma 0).
    private static func lightness(_ white: Double) -> Double {
        let linear = white <= 0.04045 ? white / 12.92 : pow((white + 0.055) / 1.055, 2.4)
        return cbrt(linear)
    }

    private static func white(_ lightness: Double) -> Double {
        let linear = min(1, pow(lightness, 3))
        return linear <= 0.0031308 ? linear * 12.92 : 1.055 * pow(linear, 1 / 2.4) - 0.055
    }

    func body(content: Content) -> some View {
        if !active || reduceMotion {
            content.foregroundStyle(Color(white: baseWhite))
        } else {
            content
                .foregroundStyle(Color(white: baseWhite))
                .overlay {
                    TimelineView(.animation) { context in
                        GeometryReader { geo in
                            band(size: geo.size, date: context.date)
                        }
                    }
                    .mask(content)
                    .allowsHitTesting(false)
                }
        }
    }

    private func band(size: CGSize, date: Date) -> some View {
        let spread = Tokens.spreadPx
        let w = Double(size.width)
        let h = max(1, Double(size.height))
        let bandWidth = 2 * w + 2 * spread
        let period = Tokens.durationMs / 1000
        // background-position 100% → 0: offset (w - W) → 0.
        let t = date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period) / period
        let offset = (w - bandWidth) * (1 - t)
        // CSS `linear-gradient(110deg, …)` over the W×h band: the gradient
        // line through the centre, `length` long, stops in px of it.
        let angle = (90 + Tokens.angleDeg) * .pi / 180
        let dx = sin(angle)
        let dy = -cos(angle)
        let length = bandWidth * abs(dx) + h * abs(dy)
        let start = UnitPoint(x: 0.5 - length / 2 * dx / bandWidth, y: 0.5 - length / 2 * dy / h)
        let end = UnitPoint(x: 0.5 + length / 2 * dx / bandWidth, y: 0.5 + length / 2 * dy / h)
        let base = Color(white: baseWhite)
        let lBase = Self.lightness(baseWhite)
        let lHigh = max(Tokens.highlightLightnessMin, lBase + Tokens.highlightLightnessBoost)
        let highlight = Color(white: Self.white(lHigh))
        let mix = Color(white: Self.white((min(1, lHigh) + lBase) / 2))
        let s = spread / length
        return LinearGradient(
            stops: [
                .init(color: base, location: 0.5 - s),
                .init(color: mix, location: 0.5 - s / 2),
                .init(color: highlight, location: 0.5),
                .init(color: mix, location: 0.5 + s / 2),
                .init(color: base, location: 0.5 + s),
            ],
            startPoint: start,
            endPoint: end
        )
        .frame(width: bandWidth, height: h)
        .offset(x: offset)
        .frame(width: w, height: h, alignment: .leading)
    }
}

// MARK: - Line

/// Whether the enclosing interactive row is pressed (its title brightens to
/// the foreground).
private struct TraceHighlightedKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var traceHighlighted: Bool {
        get { self[TraceHighlightedKey.self] }
        set { self[TraceHighlightedKey.self] = newValue }
    }
}

/// The muted-foreground grey every Trace line paints in (`#a1a1a1`).
private let traceMutedWhite = 0.6314
private let traceForegroundWhite = 0.9804

/// ONE Trace line: icon, title (truncating), optional muted description
/// (tabular figures) and an optional trailing accessory (the subagent
/// duration badge). `running` shimmers the title, `error` tints the line
/// destructive, `pending` fades it to `pendingOpacity`.
public struct TraceLine<Icon: View, Accessory: View>: View {
    let title: String
    let description: String?
    let status: TraceStatus?
    let size: TraceSize
    let icon: Icon
    let accessory: Accessory

    @Environment(\.traceHighlighted) private var highlighted
    @ScaledMetric(relativeTo: .caption) private var scale: CGFloat = 1

    public init(
        title: String,
        description: String? = nil,
        status: TraceStatus? = nil,
        size: TraceSize = .default,
        @ViewBuilder icon: () -> Icon,
        @ViewBuilder accessory: () -> Accessory
    ) {
        self.title = title
        self.description = description
        self.status = status
        self.size = size
        self.icon = icon()
        self.accessory = accessory()
    }

    private var g: Trace.LineGeometry { size.geometry }
    private var error: Bool { status == .error }

    public var body: some View {
        HStack(spacing: g.gap * scale) {
            // An `EmptyView` icon = a line whose glyph is drawn by its host
            // (the issue timeline's gutter).
            if Icon.self != EmptyView.self {
                icon
                    .frame(width: g.icon * scale, height: g.icon * scale)
                    .foregroundStyle(error ? DesignTokens.Palette.destructive : DesignTokens.Palette.mutedForeground)
            }
            titleText
            if let description {
                Text(verbatim: description)
                    .font(.system(size: g.fontSize * scale).monospacedDigit())
                    .foregroundStyle(
                        error
                            ? DesignTokens.Palette.destructive.opacity(Trace.Geometry.descriptionAlpha)
                            : DesignTokens.Palette.mutedForeground.opacity(Trace.Geometry.descriptionAlpha)
                    )
                    .lineLimit(1)
                    .fixedSize()
            }
            accessory
        }
        .frame(height: g.lineHeight * scale)
        .opacity(status == .pending ? Trace.Geometry.pendingOpacity : 1)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var titleText: some View {
        let text = Text(verbatim: title)
            .font(.system(size: g.fontSize * scale))
            .lineLimit(1)
            .truncationMode(.tail)
        if error {
            text.foregroundStyle(DesignTokens.Palette.destructive)
        } else {
            text.modifier(TraceShimmer(
                active: status == .running,
                baseWhite: highlighted ? traceForegroundWhite : traceMutedWhite
            ))
        }
    }
}

public extension TraceLine where Icon == AppIcon, Accessory == EmptyView {
    /// A line off an ExpCore model, its concept icon drawn from the registry.
    init(_ model: TraceLineModel, size: TraceSize = .default) {
        let g = size.geometry
        self.init(
            title: model.title, description: model.description, status: model.status, size: size,
            icon: { AppIcon(TraceIcons.glyph(model.icon), size: g.icon) },
            accessory: { EmptyView() }
        )
    }
}

public extension TraceLine where Accessory == EmptyView {
    init(
        title: String, description: String? = nil, status: TraceStatus? = nil,
        size: TraceSize = .default, @ViewBuilder icon: () -> Icon
    ) {
        self.init(
            title: title, description: description, status: status, size: size,
            icon: icon, accessory: { EmptyView() }
        )
    }
}

/// The subagent line's duration: shadcn's outline Badge at the line's scale.
public struct TraceBadge: View {
    let text: String
    let size: TraceSize

    @ScaledMetric(relativeTo: .caption) private var scale: CGFloat = 1

    public init(_ text: String, size: TraceSize = .default) {
        self.text = text
        self.size = size
    }

    public var body: some View {
        let small = size == .sm
        Text(verbatim: text)
            .font(.system(size: (small ? 10 : 12) * scale, weight: .medium).monospacedDigit())
            .foregroundStyle(DesignTokens.Palette.foreground)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, (small ? 6 : 8) * scale)
            .frame(height: (small ? 16 : 20) * scale)
            .overlay(Capsule().strokeBorder(DesignTokens.Palette.border, lineWidth: 1))
    }
}

// MARK: - Collapsible

/// The interactive row's press state, handed to the line through the
/// environment (the title brightens while pressed).
private struct TracePressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.environment(\.traceHighlighted, configuration.isPressed)
    }
}

/// A Trace line that discloses content: the chevron after the line rotates
/// 90° (pointing down) while open over motion `standard`; the whole row is
/// the toggle. The content sits `contentTop` below the row over a hairline
/// rail centred under the icon, inset `contentInset` / `contentTrailing`.
public struct TraceCollapsible<Line: View, Content: View>: View {
    let size: TraceSize
    let line: Line
    let content: Content
    let external: Binding<Bool>?
    let accessibilityId: String?

    @State private var internalOpen: Bool
    @Environment(\.motion) private var motion
    @ScaledMetric(relativeTo: .caption) private var scale: CGFloat = 1

    public init(
        size: TraceSize = .default,
        initiallyOpen: Bool = false,
        open: Binding<Bool>? = nil,
        accessibilityId: String? = nil,
        @ViewBuilder line: () -> Line,
        @ViewBuilder content: () -> Content
    ) {
        self.size = size
        self.external = open
        self.accessibilityId = accessibilityId
        self._internalOpen = State(initialValue: initiallyOpen)
        self.line = line()
        self.content = content()
    }

    private var isOpen: Bool { external?.wrappedValue ?? internalOpen }

    private func toggle() {
        withAnimation(motion.standard) {
            if let external {
                external.wrappedValue.toggle()
            } else {
                internalOpen.toggle()
            }
        }
    }

    public var body: some View {
        let g = size.geometry
        VStack(alignment: .leading, spacing: 0) {
            Button(action: toggle) {
                HStack(spacing: g.chevronGap * scale) {
                    line
                    AppIcon(AppIcons.traceChevron, size: g.icon)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(TracePressStyle())
            .accessibilityAddTraits(.isButton)
            .accessibilityValue(isOpen ? "Expanded" : "Collapsed")
            .accessibilityIdentifier(accessibilityId ?? "")
            if isOpen {
                VStack(alignment: .leading, spacing: g.lineGap * scale) {
                    content
                }
                .font(.system(size: g.fontSize * scale))
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, g.contentInset * scale)
                .padding(.trailing, g.contentTrailing)
                .overlay(alignment: .leading) {
                    Rectangle()
                        .fill(DesignTokens.Palette.border)
                        .frame(width: g.railWidth)
                        .padding(.leading, (g.icon * scale) / 2 - 1)
                }
                .padding(.top, g.contentTop)
                .padding(.bottom, g.contentBottom)
                .transition(.opacity)
            }
        }
    }
}

/// Lines stacked in ONE Trace sit `lineGap` apart.
public struct TraceStack<Content: View>: View {
    let size: TraceSize
    let content: Content

    @ScaledMetric(relativeTo: .caption) private var scale: CGFloat = 1

    public init(size: TraceSize = .default, @ViewBuilder content: () -> Content) {
        self.size = size
        self.content = content()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: size.geometry.lineGap * scale) {
            content
        }
    }
}
