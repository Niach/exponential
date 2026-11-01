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
    /// The thread header's waiting pick, for an embedded session page.
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
