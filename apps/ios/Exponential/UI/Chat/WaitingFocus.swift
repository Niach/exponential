import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1562 (waiting-cards.json CHIPS): a pick from the waiting menu — the
/// card to scroll to and highlight. `at` = when it was picked: a page that
/// mounts later (the Conversation shown first) still honours a fresh pick,
/// never a stale one.
struct WaitingFocus: Equatable {
    let id: String
    let at: Date

    static let freshness: TimeInterval = 2
}

private struct WaitingFocusKey: EnvironmentKey {
    static let defaultValue: WaitingFocus? = nil
}

extension EnvironmentValues {
    /// The chat header's waiting pick, for an embedded session page.
    var waitingFocus: WaitingFocus? {
        get { self[WaitingFocusKey.self] }
        set { self[WaitingFocusKey.self] = newValue }
    }
}

/// The ask-dock tint of a waiting card's kind: a plan's primary, a
/// question's yellow (THE ask card's `tint`).
enum WaitingCardTint {
    static func color(_ kind: AskDock.Kind) -> Color {
        kind == .plan ? DesignTokens.Palette.primary : DesignTokens.Semantic.yellow
    }
}

extension View {
    /// The picked card's frame: the 2pt focus-ring accent, faded out by the
    /// host after `constants.highlightMs`.
    func waitingHighlight(_ on: Bool) -> some View {
        overlay {
            RoundedRectangle(cornerRadius: GlassTokens.cardRadius, style: .continuous)
                .strokeBorder(DesignTokens.Palette.ring, lineWidth: 2)
                .opacity(on ? 1 : 0)
                .allowsHitTesting(false)
        }
    }
}

extension View {
    /// waiting-cards.json DEEP LINKS: reports this row's frame (the scroll
    /// view's space) while it is the pending link's target; nil once it
    /// leaves the lazy stack. Always attached, so the row's identity (and a
    /// card's draft) never changes when it becomes the target.
    func deepLinkRow(_ target: Bool, frame: Binding<CGRect?>) -> some View {
        modifier(DeepLinkRowFrame(target: target, frame: frame))
    }
}

/// `deepLinkRow`: the geometry reports CHANGES only, so a row that never
/// moves reports its last frame the moment it becomes the target. The frame
/// is kept by reference: a scroll never re-renders the row.
private struct DeepLinkRowFrame: ViewModifier {
    let target: Bool
    @Binding var frame: CGRect?
    @State private var last = LastFrame()

    private final class LastFrame {
        var rect: CGRect?
    }

    func body(content: Content) -> some View {
        content
            .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .scrollView) }) { rect in
                last.rect = rect
                if target { frame = rect }
            }
            .onChange(of: target) { _, target in
                if target { frame = last.rect }
            }
            .onDisappear { if target { frame = nil } }
    }
}

/// The scroll view's visible band (the scroll view's space) as last laid
/// out; held by reference so reading it never re-renders the transcript.
private final class DeepLinkViewport {
    var top: CGFloat = 0
    var height: CGFloat?
    /// The scroll view's whole height, its insets included (the band a
    /// `scrollTo` anchor is a unit point of).
    var container: CGFloat?
    var atTop = false
    var atBottom = false
    /// The target row's frame as last reported: a deferred step reads it
    /// here, never from the copy of the modifier that scheduled it.
    var target: CGRect?
    /// #EXP-2048: the link `target` was reported for: a new link never reads
    /// the previous target's frame (a row it never laid out would read as
    /// already centred, and the link rang without scrolling).
    var targetKey: String?
    /// #EXP-1998: the row the link last landed on, and when: the keyboard
    /// raised by the find field resizes the list just after a FIRST landing
    /// (it keeps its top, so the row slid under the keyboard); it is
    /// centred again while the resize lands.
    var landed: String?
    var landedAt = Date.distantPast
}

/// waiting-cards.json DEEP LINKS: THE pending deep link of a transcript.
/// A fresh `WaitingFocus` is taken ONCE (`taken`) into `link`; every layout
/// and arrival then runs `WaitingCards.DeepLink.step` (deferred off the
/// layout pass, the EXP-943 rule): page ONE older page in, reveal a lazy row,
/// scroll it to the centre, ring it (`highlightMs`). While it is pending the
/// host's follow pin stays off (`onTake`); the reader's own scroll drops it.
struct DeepLinkScroll: ViewModifier {
    let focus: WaitingFocus?
    @Binding var taken: WaitingFocus?
    @Binding var link: WaitingCards.DeepLink?
    @Binding var highlight: String?
    /// The target row's frame (`deepLinkRow`), nil = not laid out.
    let targetFrame: CGRect?
    let proxy: ScrollViewProxy
    /// The view's rows and cards have loaded once.
    let ready: Bool
    /// The target is among the rows the transcript draws.
    let found: (String) -> Bool
    /// Older rows can still page in.
    var more = false
    /// Moves when rows land (a page arriving).
    var arrival = ""
    /// One older page (the transcript's Load earlier); false = none came.
    var page: () -> Bool = { false }
    /// #EDE-130: centres the target when it is drawn under another id (a
    /// find key's transcript row, an `Int`: the proxy matches ids by their
    /// own type); nil = `proxy.scrollTo(target)` (a card).
    var centre: ((String, UnitPoint) -> Void)? = nil
    /// The link was taken: stop following the newest row.
    var onTake: () -> Void = {}
    /// The link settled `missing`: back to the newest rows.
    var onMissing: () -> Void = {}

    @Environment(\.motion) private var motion
    @State private var viewport = DeepLinkViewport()
    @State private var queued = false
    @State private var animatingUntil = Date.distantPast

    /// #EXP-1998: how long after a landing a resized list centres it again.
    private static let recentreWindow: TimeInterval = 1

    private static func nowMs() -> Double { Date().timeIntervalSince1970 * 1000 }

    @ViewBuilder
    func body(content: Content) -> some View {
        let tracked = content
            .onChange(of: focus, initial: true) { _, focus in take(focus) }
            // A link another list took (the transcript, before the no-feed
            // placeholder replaced it on a cold open) runs on here: a page
            // that list asked for is not this one's to wait on.
            .onAppear {
                guard var current = link else { return }
                if current.paging {
                    current.pageLanded()
                    link = current
                }
                schedule()
            }
            .onChange(of: targetFrame, initial: true) { _, frame in
                viewport.target = frame
                viewport.targetKey = link?.target
                schedule()
            }
            // The scroll geometry reports CHANGES only: a list that never
            // moves (the no-feed placeholder's cards) still has a height.
            .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { height in
                guard viewport.height == nil else { return }
                viewport.height = height
                if link != nil { schedule() }
            }
            .onChange(of: ready) { _, _ in schedule() }
            .onChange(of: more) { _, _ in schedule() }
            .onChange(of: arrival) { _, _ in
                guard var current = link, current.paging else { return schedule() }
                current.pageLanded()
                link = current
                schedule()
            }
        if #available(iOS 18.0, *) {
            tracked
                .onScrollGeometryChange(for: [CGFloat].self) { geo in
                    [
                        geo.contentInsets.top,
                        geo.containerSize.height - geo.contentInsets.top - geo.contentInsets.bottom,
                        geo.contentOffset.y + geo.contentInsets.top,
                        geo.contentSize.height - geo.visibleRect.maxY,
                        geo.containerSize.height,
                    ]
                } action: { _, metrics in
                    let resized = viewport.container.map { abs($0 - metrics[4]) > 0.5 } ?? false
                    viewport.container = metrics[4]
                    viewport.top = metrics[0]
                    viewport.height = metrics[1]
                    viewport.atTop = metrics[2] <= 0.5
                    viewport.atBottom = metrics[3] <= 0.5
                    if link != nil { schedule() }
                    if resized, link == nil, let landed = viewport.landed,
                       Date().timeIntervalSince(viewport.landedAt) < Self.recentreWindow {
                        Task { @MainActor in scrollToCentre(landed) }
                    }
                }
                .onScrollPhaseChange { _, phase in
                    // The reader's own drag drops the link.
                    if phase == .tracking || phase == .interacting {
                        link = nil
                        viewport.landed = nil
                    }
                    if phase == .idle, link != nil { schedule() }
                }
        } else {
            tracked
        }
    }

    private func take(_ focus: WaitingFocus?) {
        guard let focus, focus != taken, Date().timeIntervalSince(focus.at) < WaitingFocus.freshness else { return }
        taken = focus
        if viewport.targetKey != focus.id { viewport.target = nil }
        link = WaitingCards.DeepLink(target: focus.id, takenAtMs: Self.nowMs())
        onTake()
        schedule()
        // Past `deepLinkMs` the link gives up even if nothing lays out again.
        schedule(afterMs: Double(WaitingCards.Constants.deepLinkMs) + 16)
    }

    /// Run a step after this layout pass (one per pass), or after `afterMs`.
    private func schedule(afterMs: Double = 0) {
        if afterMs > 0 {
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(Int(afterMs)))
                schedule()
            }
            return
        }
        guard link != nil, !queued else { return }
        queued = true
        Task { @MainActor in
            queued = false
            run()
        }
    }

    private func run() {
        guard ready, var current = link else { return }
        // A centring scroll in flight lands first.
        guard Date() >= animatingUntil else { return }
        let target = current.target
        let centre = viewport.top + (viewport.height ?? 0) / 2
        let distance = viewport.height == nil ? nil : viewport.target.map { Double($0.midY - centre) }
        let atEdge = distance.map { ($0 < 0 && viewport.atTop) || ($0 > 0 && viewport.atBottom) } ?? false
        let action = current.step(
            .init(found: found(target), distance: distance, atEdge: atEdge, more: more, loaded: arrival),
            nowMs: Self.nowMs()
        )
        // #FEED-298: a landed page restarted the clock; give up past it.
        if current.takenAtMs != link?.takenAtMs {
            schedule(afterMs: Double(WaitingCards.Constants.deepLinkMs) + 16)
        }
        link = current.settled ? nil : current
        switch action {
        case .none:
            break
        case .reveal:
            scrollToCentre(target)
            schedule(afterMs: 50)
        case .scroll:
            let duration = DesignTokens.Motion.Duration.standard
            animatingUntil = Date().addingTimeInterval(duration)
            withAnimation(motion.standard) { scrollToCentre(target) }
            schedule(afterMs: duration * 1000 + 50)
        case .page:
            if !page() {
                current.pageLanded()
                link = current
                schedule(afterMs: 16)
            }
        case .ring:
            viewport.landed = target
            viewport.landedAt = Date()
            ring(target)
        case .missing:
            onMissing()
        }
    }

    private func scrollToCentre(_ target: String) {
        let anchor = centreAnchor()
        if let centre { centre(target, anchor) } else { proxy.scrollTo(target, anchor: anchor) }
    }

    /// #EXP-1998: `scrollTo` aligns the row's anchor point with the same
    /// point of the WHOLE scroll view, insets included (the header band, the
    /// keyboard, the composer), so `.center` lands it off the visible band's
    /// centre: the anchor that puts the row's middle at that centre.
    private func centreAnchor() -> UnitPoint {
        guard let container = viewport.container, let height = viewport.height,
              let row = viewport.target?.height, container > row else { return .center }
        let y = (viewport.top + height / 2 - row / 2) / (container - row)
        return UnitPoint(x: 0.5, y: min(max(y, 0), 1))
    }

    private func ring(_ id: String) {
        // The ring comes in fast and fades out slowly (web 150 / 500 ms).
        withAnimation(.easeOut(duration: 0.15)) { highlight = id }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(WaitingCards.Constants.highlightMs))
            guard highlight == id else { return }
            withAnimation(.easeOut(duration: 0.5)) { highlight = nil }
        }
    }
}

/// #EXP-1785 (waiting-cards.json CHIPS, phones): THE waiting button at the
/// start of the session header's trailing slot — ONE icon-only button,
/// `icons.waiting` in the question tint, the count beside it muted; it
/// opens `ChatWaitingSheet`. Not drawn when nothing waits.
struct ChatWaitingButton: View {
    let count: Int
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                AppIcon(
                    AppIcons.conceptOrName(WaitingCards.Icons.waiting) ?? AppIcons.uiHelp,
                    size: CGFloat(Chats.Shell.ChatHeader.glyph), weight: .medium
                )
                .foregroundStyle(WaitingCardTint.color(.question))
                Text("\(count)")
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 6)
            .frame(height: CGFloat(Chats.Shell.ChatHeader.button))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .fixedSize()
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(WaitingCards.waitingCountLabel(count))
        .accessibilityIdentifier(WaitingCards.TestIds.button)
    }
}

/// #EXP-1562 / #EXP-1785 (waiting-cards.json CHIPS): the waiting button's
/// Menu — on phones THE bottom sheet, headed `copy.waiting`: THE MenuRow per
/// waiting card in Conversation order (the kind icon tinted, the chip label,
/// the kind's ask-dock label muted). A pick hands the card back; the host
/// goes to it (`waitingTarget`) once the sheet has dismissed.
struct ChatWaitingSheet: View {
    let chips: [WaitingCards.Chip]
    let onPick: (String) -> Void

    var body: some View {
        GlassSheetChrome(title: WaitingCards.Copy.waiting) {
            VStack(spacing: 0) {
                ForEach(chips) { chip in
                    GlassPickerSheetRow(
                        item: PickerItem(
                            value: chip.id,
                            label: chip.label,
                            icon: AppIcons.conceptOrName(chip.icon) ?? chip.icon,
                            color: WaitingCardTint.color(chip.kind)
                        ),
                        checked: .none,
                        style: .check,
                        content: AnyView(HStack(spacing: 8) {
                            Text(chip.label)
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.primary))
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Text(AskDock.label(chip.kind))
                                .font(.caption)
                                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                                .lineLimit(1)
                                .fixedSize()
                        })
                    ) {
                        onPick(chip.id)
                    }
                    .accessibilityIdentifier("\(WaitingCards.TestIds.menu)-row")
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(WaitingCards.TestIds.menu)
        }
    }
}
