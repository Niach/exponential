import ExpCore
import ExpUI
import SwiftUI

/// N-12 / S-3 (composer-menu.json): the expanded steer composer's ONE tool
/// row: the `+` — the composer's ONLY tool — then the context ring beside
/// Send ⇄ Stop. No options line, no inline device / model pins, no
/// image/file/#/emoji/mention buttons: the model is the "+" Model row
/// (surface session | agent), a running session never moves device or
/// login here. With `controls` the `+` opens THE "+" sheet
/// (`ThreadComposerPlusMenu`, presented by `SessionComposerCard`); a bare
/// box (no controls) opens the Photo | File sub-choice directly. The ring
/// hides until context data lands and opens the Usage sheet (EXP-1150).
/// Rides `GlassComposer`'s `tools:` slot, so the send button stays the
/// card's own.
struct SessionComposerFooter: View {
    /// The session's own controls: what the "+" sheet's Model row reads and
    /// picks, and the usage ring.
    struct Controls {
        /// `AgentSessionModel.composerModelValue` (live, else the row, else
        /// the device/contract defaults) — nil = no Model row value.
        let modelValue: String?
        /// The run's agent (its models).
        let agent: String
        /// EXP-877: a pick is a plain `/model <alias>` message.
        let onPickModel: (String) -> Void
        let usageFraction: Double?
        let usageSeverity: AgentUsageSeverity
        let onUsage: () -> Void
        /// The session's row ENDED: no model switch (web disables the
        /// picker on `status === 'ended'`).
        var ended: Bool = false
        /// composer-menu.json `surface`: a session's composer or an agent
        /// chat's.
        var surface: ComposerMenu.Surface = .session
        /// The team whose issues the "+" Issue page searches.
        var teamId: String?

        /// composer-menu.json `model`: a session or an agent chat whose
        /// agent switches models (always for claude and codex), never ended.
        var canPickModel: Bool { !ended && (agent == "claude" || agent == "codex") }

        /// The "+" conditions of this composer.
        var menuConditions: ComposerMenu.Conditions {
            ComposerMenu.Conditions(model: canPickModel)
        }
    }

    /// nil = the bare box (the `+` = Photo | File).
    let controls: Controls?
    let attachEnabled: Bool
    /// Wave D: the `+` opens the Photo | File sub-choice (iOS only, see
    /// `SteerAttachChoiceMenu`): the photo picker or the Files importer.
    let onPhoto: () -> Void
    let onFile: () -> Void
    /// Owned by the host so an open menu (its cover resigns the field's
    /// focus) never reads as an idle composer and folds it away.
    @Binding var attachMenuOpen: Bool
    /// S-3: the `+` opens THE "+" sheet (with controls).
    var onPlus: (() -> Void)?

    @State private var attachAnchor: CGRect = .zero

    var body: some View {
        HStack(spacing: GlassComposerTokens.toolSpacing) {
            // EXP-850 §13: the steer composers attach with the `ui-add` (plus)
            // concept ×4; `editor-image` stays the comment/description glyph.
            GlassComposerToolButton(
                AppIcons.uiAdd,
                accessibilityLabel: onPlus == nil ? "Add file or image" : ComposerMenu.plusLabel,
                enabled: onPlus != nil || attachEnabled
            ) {
                if let onPlus {
                    onPlus()
                    return
                }
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

            // No context data yet = no ring (web/Android draw nothing then).
            if let controls, controls.usageFraction != nil {
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
        .layoutPriority(1)
    }
}

/// EXP-1252: the session's Device pin — what it needs to draw and act.
struct SessionDevicePin {
    /// The current device's name.
    let label: String
    /// The current device's glyph (`DeviceIconDisplay`); nil = none known.
    let icon: String?
    /// The launch composer's devices for this team, refused ones disabled
    /// with their reason as the description.
    let rows: [DevicePickerDevice]
    let currentDeviceId: String
    /// The whole pin's refusal (`SessionDeviceMove.blockedReason`): a
    /// chevron-less label that says why when tapped.
    let blockedReason: String?
    /// A request is in flight.
    var sending: Bool = false
    /// Another device = move; the current one does nothing.
    let onPick: (String) -> Void
}

extension DevicePickerDevice {
    /// EXP-1252: a move target — the ONE device row bridge, a refused row
    /// disabled with its reason as the description.
    init(moveRow row: SessionDeviceMove.Row) {
        let base = DevicePickerDevice(row.device)
        self.init(
            id: base.id,
            name: base.name,
            icon: base.icon,
            description: row.blockedReason ?? base.description,
            disabled: row.blockedReason != nil
        )
    }
}
