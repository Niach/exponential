import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// THE ask dock ×iOS (`fixtures/ask-dock.json` ×4: web `@exp/ui`
/// `ask-dock.tsx`, desktop `ask_dock.rs`, Android `AskDock.kt`): the
/// question/plan cards WAITING on the viewer leave the transcript and take
/// the composer's slot, in the session view AND the thread alike. One card =
/// THE big card (`QuestionCard`, the host draws it); 2+ = compact cards
/// stacked `stackGap` apart, oldest first; a tap FOCUSES one: the dock grows
/// into its big card (height over motion `standard`, the new content fading
/// in decelerated; reduce motion = the fade only), and the back arrow
/// leading its header returns to the stack. An answered card leaves
/// `pending`, so a focused one falls back to the stack by itself (or the
/// last card's `single`). The model is ExpCore `AskDock.dock`.
struct AskDockView<Card: View>: View {
    let pending: [AskDock.Pending]
    /// The big card for an id: the back handler is set when it was focused
    /// out of a stack (the host hands it to `QuestionCard(onBack:)`), the cap
    /// is the body's max height (`QuestionCard(bodyMaxHeight:)`).
    let card: (_ id: String, _ onBack: (() -> Void)?, _ bodyMaxHeight: CGFloat) -> Card

    @State private var focusedId: String?
    @Environment(\.motion) private var motion

    /// `initialFocusedId` = the card focused on mount (the styleguide's
    /// focused state); a live host starts on the stack.
    init(
        pending: [AskDock.Pending],
        initialFocusedId: String? = nil,
        @ViewBuilder card: @escaping (_ id: String, _ onBack: (() -> Void)?, _ bodyMaxHeight: CGFloat) -> Card
    ) {
        self.pending = pending
        self.card = card
        _focusedId = State(initialValue: initialFocusedId)
    }

    /// The big card's body cap: `bodyMaxHeightFraction` of the screen.
    static var bodyMaxHeight: CGFloat {
        UIScreen.main.bounds.height * AskDock.bodyMaxHeightFraction
    }

    private var layout: AskDock.Layout { AskDock.dock(pending: pending, focusedId: focusedId) }

    var body: some View {
        let layout = layout
        // The fixture's test ids, verbatim ×4: `ask-dock` on the whole dock,
        // `ask-dock-card` on the big card's wrapper, `ask-dock-row` per
        // stacked card. Each wrapper is an explicit accessibility CONTAINER
        // (`children: .contain`), so its id never folds onto the card's own
        // (`agent-feed-question`, `thread-plan-card`, which the shot walks
        // and the store slide's pop-out rect look up). The uniqueness
        // suffix lives in the view identity (`.id`), never the test id.
        VStack(spacing: 0) {
            if let cardId = layout.cardId {
                VStack(spacing: 0) {
                    card(cardId, layout.back ? { setFocus(nil) } : nil, Self.bodyMaxHeight)
                }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(AskDock.TestIds.card)
                    .modifier(AskDockEnter())
                    .id("card:\(cardId)")
            } else if !layout.rows.isEmpty {
                VStack(spacing: AskDock.stackGap) {
                    ForEach(layout.rows) { row in
                        AskDockStackCard(row: row) { setFocus(row.id) }
                    }
                }
                .modifier(AskDockEnter())
                .id("stack")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AskDock.TestIds.dock)
    }

    /// The height animates over `standard` (nil under reduce motion: the
    /// swapped content's own fade is all that moves).
    private func setFocus(_ id: String?) {
        withAnimation(motion.standard) { focusedId = id }
    }
}

/// The swapped dock content's entrance (the web `motion-enter` recipe):
/// it fades in decelerated over `standard`; under reduce motion it still
/// fades (opacity only), just without the curve's travel.
private struct AskDockEnter: ViewModifier {
    @State private var shown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .onAppear {
                let animation = reduceMotion
                    ? Animation.linear(duration: DesignTokens.Motion.Duration.fast)
                    : DesignTokens.Motion.Ease.decelerate.animation(duration: DesignTokens.Motion.Duration.standard)
                withAnimation(animation) { shown = true }
            }
    }
}

/// The focused card's back arrow (web ghost `icon-xs`, `-my-1 -ml-1`):
/// a 24pt ghost button with the 14pt `ui-back` glyph, LEADING the header.
struct AskDockBackButton: View {
    let action: () -> Void

    var body: some View {
        GhostIconButton(
            AppIcons.uiBack,
            accessibilityLabel: AskDock.Copy.back,
            size: 24,
            glyphSize: 14,
            action: action
        )
        .padding(.vertical, -4)
        .padding(.leading, -4)
        .accessibilityIdentifier(AskDock.TestIds.back)
    }
}

/// A compact card in the stack: `QuestionCard`'s chrome (padding 12, gap 8,
/// the 14pt kind glyph, the 12pt medium tinted label) with the asking
/// session's name as the muted caption, over the one-line title and the
/// open chevron. The whole card is the tap target.
struct AskDockStackCard: View {
    let row: AskDock.Row
    let onOpen: () -> Void

    private var plan: Bool { row.kind == .plan }

    private var tint: Color {
        plan ? DesignTokens.Palette.primary : DesignTokens.Semantic.yellow
    }

    var body: some View {
        Button(action: onOpen) {
            HStack(alignment: .top, spacing: 8) {
                AppIcon(plan ? AppIcons.codingPlan : AppIcons.uiHelp, size: 14)
                    .foregroundStyle(tint)
                    .padding(.top, 1)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(row.label)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(tint)
                            .lineLimit(1)
                            .fixedSize()
                        if let caption = row.caption, !caption.isEmpty {
                            Text(caption)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                                .truncationMode(.tail)
                        }
                    }
                    HStack(spacing: 8) {
                        Text(row.title)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(0.9))
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        AppIcon(AppIcons.uiChevronRight, size: 14)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(12)
            .glassCard(isOpaque: true)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(row.label): \(row.title)")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier(AskDock.TestIds.row)
    }
}

// MARK: - Plan options ×4

extension AskDock {
    /// THE plan card's rows (`planOptions`) as `QuestionCard` options: the
    /// first is the primary approve, `reject` expands into the feedback field
    /// (`AgentFeed.planRejectKey`).
    static var planQuestionOptions: [AgentQuestionOption] {
        planOptions.map { option in
            AgentQuestionOption(label: option.label, key: option.key, description: option.description)
        }
    }

    /// The picked plan option for a key (nil = unknown).
    static func planOption(key: String?) -> PlanOption? {
        planOptions.first { $0.key == key }
    }
}
