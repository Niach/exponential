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

/// Scrolls a transcript to a fresh `WaitingFocus` (centred), highlights the
/// card and fades the ring out after `constants.highlightMs`. `onScroll`
/// unpins the host's follow state first.
struct WaitingFocusScroll: ViewModifier {
    let focus: WaitingFocus?
    let proxy: ScrollViewProxy
    @Binding var highlight: String?
    var onScroll: () -> Void = {}

    @Environment(\.motion) private var motion

    func body(content: Content) -> some View {
        content.onChange(of: focus, initial: true) { _, focus in
            guard let focus, Date().timeIntervalSince(focus.at) < WaitingFocus.freshness else { return }
            onScroll()
            Task { @MainActor in
                // A page that just mounted lays its rows out first.
                try? await Task.sleep(for: .milliseconds(120))
                withAnimation(motion.standard) { proxy.scrollTo(focus.id, anchor: .center) }
                // The ring comes in fast and fades out slowly (web 150 / 500 ms).
                withAnimation(.easeOut(duration: 0.15)) { highlight = focus.id }
                try? await Task.sleep(for: .milliseconds(WaitingCards.Constants.highlightMs))
                guard highlight == focus.id else { return }
                withAnimation(.easeOut(duration: 0.5)) { highlight = nil }
            }
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
