import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the expanded steer composer's footer row — desktop
/// `steer-composer.tsx` in concept: `Plan mode` (blue while the run is in
/// plan mode, read-only: EXP-790 keeps plan mode launch-time) · `+` attach
/// (wave D: Photo | File) ·
/// spacer · the model pill (the session's `model` config option; picking one
/// SENDS `/model <alias>` as a plain message, EXP-877; codex gets a plain
/// label; before the relay reports one it falls back to the row/device model) · the usage ring, which
/// opens the Usage sheet (EXP-1150: the face switcher that used to sit beside
/// it is gone — the faces are the Work screen's tab strip, which this composer
/// never covers). Rides `GlassComposer`'s `tools:` slot, so the send button
/// stays the card's own. A thread's box carries the same controls for its
/// ROOT session, minus the plan chip on a dispatcher root.
struct SessionComposerFooter: View {
    /// The session view's own controls: the plan chip, the model pill and
    /// the usage ring.
    struct Controls {
        let planModeActive: Bool
        /// `AgentSessionModel.composerModelValue` (live, else the row, else
        /// the device/contract defaults) — nil hides the pill.
        let modelValue: String?
        /// The run's agent — claude gets the picker, anything else a label.
        let agent: String
        let onPickModel: (String) -> Void
        let usageFraction: Double?
        let usageSeverity: AgentUsageSeverity
        let onUsage: () -> Void
        /// A dispatcher root (`sessions.dispatch`) has no plan mode of its
        /// own: its children take the launch's picks, so no plan chip.
        var showsPlan: Bool = true
    }

    /// nil = the bare box (the `+` alone).
    let controls: Controls?
    let attachEnabled: Bool
    /// Wave D: the `+` opens the Photo | File sub-choice (iOS only, see
    /// `SteerAttachChoiceMenu`): the photo picker or the Files importer.
    let onPhoto: () -> Void
    let onFile: () -> Void
    /// Owned by the host so an open menu (its cover resigns the field's
    /// focus) never reads as an idle composer and folds it away.
    @Binding var attachMenuOpen: Bool

    @State private var attachAnchor: CGRect = .zero

    var body: some View {
        if let controls, controls.showsPlan {
            Text(AgentFeed.planModeFooterLabel)
                .font(.caption.weight(.medium))
                .foregroundStyle(
                    controls.planModeActive ? DesignTokens.Semantic.blue : .white.opacity(TextOpacity.tertiary)
                )
                .lineLimit(1)
                .padding(.trailing, 4)
                .accessibilityIdentifier("session-plan-mode")
        }

        // EXP-850 §13: the steer composers attach with the `ui-add` (plus)
        // concept ×4; `editor-image` stays the comment/description glyph.
        GlassComposerToolButton(
            AppIcons.uiAdd,
            accessibilityLabel: "Add file or image",
            enabled: attachEnabled
        ) {
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) { attachMenuOpen = true }
        }
        .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
            attachAnchor = frame
        }
        .steerAttachChoiceMenu(
            isPresented: $attachMenuOpen,
            anchor: attachAnchor,
            onPhoto: onPhoto,
            onFile: onFile
        )

        Spacer(minLength: 0)

        if let controls {
            modelPill(controls)

            Button(action: controls.onUsage) {
                ContextRing(fraction: controls.usageFraction, severity: controls.usageSeverity)
                    .frame(
                        width: GlassComposerTokens.toolHitSize,
                        height: GlassComposerTokens.toolHitSize
                    )
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Usage")
            .accessibilityIdentifier("session-context-ring")
        }
    }

    @ViewBuilder
    private func modelPill(_ controls: Controls) -> some View {
        let agent = controls.agent
        if let modelValue = controls.modelValue {
            if agent == "claude" {
                GlassMenu {
                    ForEach(LaunchVocabulary.modelValues(for: agent), id: \.self) { value in
                        GlassMenuItem(LaunchVocabulary.modelLabel(value)) {
                            controls.onPickModel(value)
                        }
                    }
                } label: {
                    OptionPillLabel(text: LaunchVocabulary.modelLabel(modelValue))
                }
                .accessibilityLabel("Model")
                .accessibilityIdentifier("session-model-pill")
            } else {
                OptionPillLabel(text: LaunchVocabulary.modelLabel(modelValue), chevron: false)
                    .accessibilityLabel("Model")
                    .accessibilityIdentifier("session-model-pill")
            }
        }
    }
}
