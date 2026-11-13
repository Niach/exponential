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
        onGeometryChange(for: CGRect?.self, of: { target ? $0.frame(in: .scrollView) : nil }) { rect in
            if target { frame.wrappedValue = rect }
        }
        .onDisappear { if target { frame.wrappedValue = nil } }
    }
}

/// The scroll view's visible band (the scroll view's space) as last laid
/// out; held by reference so reading it never re-renders the transcript.
private final class DeepLinkViewport {
    var top: CGFloat = 0
    var height: CGFloat?
    var atTop = false
    var atBottom = false
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
    /// The link was taken: stop following the newest row.
    var onTake: () -> Void = {}
    /// The link settled `missing`: back to the newest rows.
    var onMissing: () -> Void = {}

    @Environment(\.motion) private var motion
    @State private var viewport = DeepLinkViewport()
    @State private var queued = false
    @State private var animatingUntil = Date.distantPast

    private static func nowMs() -> Double { Date().timeIntervalSince1970 * 1000 }

    @ViewBuilder
    func body(content: Content) -> some View {
        let tracked = content
            .onChange(of: focus, initial: true) { _, focus in take(focus) }
            .onChange(of: targetFrame) { _, _ in schedule() }
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
                    ]
                } action: { _, metrics in
                    viewport.top = metrics[0]
                    viewport.height = metrics[1]
                    viewport.atTop = metrics[2] <= 0.5
                    viewport.atBottom = metrics[3] <= 0.5
                    if link != nil { schedule() }
                }
                .onScrollPhaseChange { _, phase in
                    // The reader's own drag drops the link.
                    if phase == .tracking || phase == .interacting { link = nil }
                    if phase == .idle, link != nil { schedule() }
                }
        } else {
            tracked
        }
    }

    private func take(_ focus: WaitingFocus?) {
        guard let focus, focus != taken, Date().timeIntervalSince(focus.at) < WaitingFocus.freshness else { return }
        taken = focus
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
        let distance = viewport.height == nil ? nil : targetFrame.map { Double($0.midY - centre) }
        let atEdge = distance.map { ($0 < 0 && viewport.atTop) || ($0 > 0 && viewport.atBottom) } ?? false
        let action = current.step(
            .init(found: found(target), distance: distance, atEdge: atEdge, more: more),
            nowMs: Self.nowMs()
        )
        link = current.settled ? nil : current
        switch action {
        case .none:
            break
        case .reveal:
            proxy.scrollTo(target, anchor: .center)
            schedule(afterMs: 50)
        case .scroll:
            let duration = DesignTokens.Motion.Duration.standard
            animatingUntil = Date().addingTimeInterval(duration)
            withAnimation(motion.standard) { proxy.scrollTo(target, anchor: .center) }
            schedule(afterMs: duration * 1000 + 50)
        case .page:
            if !page() {
                current.pageLanded()
                link = current
                schedule(afterMs: 16)
            }
        case .ring:
            ring(target)
        case .missing:
            onMissing()
        }
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
