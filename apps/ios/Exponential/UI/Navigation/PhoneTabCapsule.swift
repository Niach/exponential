import ExpCore
import ExpUI
import SwiftUI

/// A running app's own screens bar (apps.json `screens`, the BAR; #EXP-2109:
/// the floating glass capsule stays only here): each tab its face (an icon
/// over its label) on the glass capsule, the selected one on the glass-active
/// pill that SLIDES between tabs (EXP-523), the drag LENS (`shell.bar.drag`),
/// the widths (`MobileTabBarMetrics.tabWidths`).
struct PhoneTabCapsule<Tab: Hashable, Face: View>: View {
    let tabs: [Tab]
    /// The lit tab (nil = none).
    var selected: Tab?
    let label: (Tab) -> String
    let identifier: (Tab) -> String
    /// A tap, a lift, a VoiceOver activation.
    let onActivate: (Tab) -> Void
    /// The tab's icon over its label (lit, padded and framed here).
    let face: (Tab) -> Face

    init(
        tabs: [Tab],
        selected: Tab?,
        label: @escaping (Tab) -> String,
        identifier: @escaping (Tab) -> String,
        onActivate: @escaping (Tab) -> Void,
        @ViewBuilder face: @escaping (Tab) -> Face
    ) {
        self.tabs = tabs
        self.selected = selected
        self.label = label
        self.identifier = identifier
        self.onActivate = onActivate
        self.face = face
    }

    @Namespace private var tabPill
    @Environment(\.motion) private var motion
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// The press in progress (nil = none).
    @State private var press: Press?
    /// The lens: under the finger while pressed, settling after a lift.
    @State private var lens: Lens?
    /// The tab (index) under the lens' centre while pressed.
    @State private var candidate: Int?
    /// Every tab's frame (by index) in the capsule's coordinate space.
    @State private var tabFrames: [Int: CGRect] = [:]

    private struct Press {
        let pressed: Int
        var dragging = false
    }

    private struct Lens: Equatable {
        var minX: CGFloat
        var width: CGFloat
        var scale: CGFloat
    }

    private static var space: String { "phone-tab-strip" }

    private var selectedIndex: Int? { selected.flatMap { tabs.firstIndex(of: $0) } }

    var body: some View {
        TabStripLayout {
            ForEach(tabs.indices, id: \.self) { index in
                tabFace(index)
            }
        }
        .coordinateSpace(.named(Self.space))
        .background(alignment: .topLeading) { lensView }
        .onPreferenceChange(TabFramesKey.self) { frames in
            MainActor.assumeIsolated { tabFrames = frames }
        }
        .contentShape(Rectangle())
        .gesture(dragGesture)
        .animation(motion.standard, value: selectedIndex)
        .padding(MobileTabBarMetrics.pillPadding)
        .frame(maxWidth: .infinity)
        .tabBarGlass()
    }

    // MARK: - Tabs

    private var lensScale: CGFloat { reduceMotion ? 1 : MobileTabBarMetrics.lensScale }

    /// While pressed the candidate is lit; else the selected tab.
    private func isLit(_ index: Int) -> Bool {
        lens != nil && candidate != nil ? candidate == index : selectedIndex == index
    }

    private func tabFace(_ index: Int) -> some View {
        let tab = tabs[index]
        let lit = isLit(index)
        let isSelected = selectedIndex == index
        return face(tab)
            .foregroundStyle(.white.opacity(lit ? 1 : TextOpacity.secondary))
            .padding(.horizontal, MobileTabBarMetrics.tabPaddingX)
            .frame(minWidth: MobileTabBarMetrics.tabMinWidth, maxWidth: .infinity)
            .frame(height: MobileTabBarMetrics.tabHeight)
            .background {
                // Only the SELECTED tab renders the pill, carrying the shared
                // geometry id — that is what makes it travel. A press lifts it
                // (the lens takes over).
                if isSelected && lens == nil {
                    Capsule()
                        .fill(GlassTokens.fillActive)
                        .matchedGeometryEffect(id: "tab-pill", in: tabPill)
                }
            }
            .background(GeometryReader { proxy in
                Color.clear.preference(
                    key: TabFramesKey.self,
                    value: [index: proxy.frame(in: .named(Self.space))]
                )
            })
            .contentShape(Capsule())
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label(tab))
            .accessibilityAddTraits(isSelected ? [.isButton, .isSelected] : .isButton)
            .accessibilityAction { onActivate(tab) }
            .accessibilityIdentifier(identifier(tab))
    }

    // MARK: - Drag lens

    @ViewBuilder
    private var lensView: some View {
        if let lens {
            LensFace()
                .frame(width: lens.width, height: MobileTabBarMetrics.tabHeight)
                .scaleEffect(lens.scale)
                .offset(x: lens.minX)
                .allowsHitTesting(false)
        }
    }

    private var dragGesture: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .named(Self.space))
            .onChanged { value in
                if press == nil {
                    guard let pressed = tab(at: value.startLocation.x),
                          let frame = tabFrames[pressed] else { return }
                    press = Press(pressed: pressed)
                    candidate = pressed
                    // The press lifts the selection under the finger.
                    withAnimation(motion.fast) {
                        lens = Lens(minX: frame.minX, width: frame.width, scale: lensScale)
                    }
                }
                guard var current = press else { return }
                if !current.dragging {
                    let moved = hypot(value.translation.width, value.translation.height)
                    guard moved >= MobileTabBarMetrics.dragSlop else { return }
                    current.dragging = true
                    press = current
                }
                let next = candidateAt(value.location.x)
                candidate = next.tab
                // The lens moves WITH the finger: no animation while dragging.
                var still = Transaction()
                still.disablesAnimations = true
                withTransaction(still) {
                    lens = Lens(minX: next.minX, width: next.width, scale: lensScale)
                }
            }
            .onEnded { value in
                guard let current = press else { return }
                press = nil
                guard current.dragging else {
                    // A plain tap selects at once.
                    settle(on: current.pressed)
                    activate(current.pressed)
                    return
                }
                let height = MobileTabBarMetrics.tabHeight
                let y = value.location.y
                let off = y < 0 ? -y : (y > height ? y - height : 0)
                let target = candidateAt(value.location.x).tab
                if off > MobileTabBarMetrics.cancelDistance || target == nil {
                    // A lift far off the bar cancels: back to the selected tab.
                    settle(on: selectedIndex)
                } else if let target {
                    settle(on: target)
                    activate(target)
                }
            }
    }

    private func activate(_ index: Int) {
        guard tabs.indices.contains(index) else { return }
        onActivate(tabs[index])
    }

    /// The tab whose frame holds `x`, else the nearest one.
    private func tab(at x: CGFloat) -> Int? {
        if let hit = tabFrames.first(where: { $0.value.minX <= x && x <= $0.value.maxX }) {
            return hit.key
        }
        return tabFrames.min { lhs, rhs in
            distance(x, lhs.value) < distance(x, rhs.value)
        }?.key
    }

    private func distance(_ x: CGFloat, _ frame: CGRect) -> CGFloat {
        min(abs(x - frame.minX), abs(x - frame.maxX))
    }

    /// The candidate under a finger at `x` and the lens frame centred on the
    /// finger (clamped to the bar).
    private func candidateAt(_ x: CGFloat) -> (tab: Int?, minX: CGFloat, width: CGFloat) {
        let lower = tabFrames.values.map(\.minX).min() ?? 0
        let upper = tabFrames.values.map(\.maxX).max() ?? 0
        let clamped = min(max(x, lower), upper)
        let tab = tab(at: clamped)
        let width = tab.flatMap { tabFrames[$0]?.width } ?? MobileTabBarMetrics.tabMinWidth
        let minX = min(max(clamped - width / 2, lower), upper - width)
        return (tab, minX, width)
    }

    /// The lens settles over `tab` at standard / decelerate, then leaves the
    /// selected pill in charge.
    private func settle(on tab: Int?) {
        candidate = nil
        guard let tab, let frame = tabFrames[tab] else {
            lens = nil
            return
        }
        let target = Lens(minX: frame.minX, width: frame.width, scale: 1)
        if let animation = motion.decelerate(DesignTokens.Motion.Duration.standard) {
            withAnimation(animation) {
                lens = target
            } completion: {
                if press == nil { lens = nil }
            }
        } else {
            lens = nil
        }
    }
}

/// The drag lens: iOS 26 = the interactive Liquid Glass; before it the
/// glass-active pill.
private struct LensFace: View {
    var body: some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer {
                Capsule()
                    .fill(GlassTokens.fillActive)
                    .glassEffect(.regular.interactive(), in: Capsule())
            }
        } else {
            Capsule().fill(GlassTokens.fillActive)
        }
    }
}

/// The tabs' frames (by index) in the capsule's coordinate space.
private struct TabFramesKey: PreferenceKey {
    static let defaultValue: [Int: CGRect] = [:]
    static func reduce(value: inout [Int: CGRect], nextValue: () -> [Int: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

/// `shell.bar`: each tab at its natural width (label + padding, at least the
/// minimum), the rest shared evenly; the widest gives way first when the
/// labels alone do not fit (`MobileTabBarMetrics.tabWidths`).
private struct TabStripLayout: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let natural = subviews.map { $0.sizeThatFits(.unspecified).width }
        return CGSize(
            width: proposal.width ?? natural.reduce(0, +),
            height: MobileTabBarMetrics.tabHeight
        )
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let natural = subviews.map { $0.sizeThatFits(.unspecified).width }
        let widths = MobileTabBarMetrics.tabWidths(natural: natural, available: bounds.width)
        var x = bounds.minX
        for (subview, width) in zip(subviews, widths) {
            subview.place(
                at: CGPoint(x: x, y: bounds.minY),
                anchor: .topLeading,
                proposal: ProposedViewSize(width: width, height: MobileTabBarMetrics.tabHeight)
            )
            x += width
        }
    }
}

private extension View {
    /// The bar's capsule chrome: the system Liquid Glass on iOS 26+, else
    /// the OPAQUE card composite under the strong hairline (EXP-698: never a
    /// low-alpha tint the feed shows through).
    @ViewBuilder
    func tabBarGlass() -> some View {
        if #available(iOS 26.0, *) {
            self.glassEffect(.regular.tint(GlassTokens.opaqueCardFill.opacity(0.55)).interactive(), in: Capsule())
        } else {
            self
                .background(GlassTokens.opaqueCardFill, in: Capsule())
                .overlay(Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
        }
    }
}
