import ExpCore
import SwiftUI
import UIKit

// The app's OWN alert (EXP-1212): the iOS twin of the web phone layout's
// centred alert card (`packages/ui` `MOBILE_ALERT` + `AlertDialogFooter`) —
// a dimmed, blurred scrim, a centred glass card, a left-aligned title (the
// question; an optional muted body) and ONE compact row of `GlassPill` `.md`
// buttons: a quiet destructive text answer set apart on the leading edge, the
// rest trailing (Thunderbird's save prompt). When the row does not fit it
// STACKS: one natural-width pill per line, trailing-aligned, the default
// first and Cancel last (fixture `prompts.json` `_comment`); never a two-row
// hybrid, never full-width blocks. Never the system `.alert` /
// `.confirmationDialog`: iOS 26+ draws the latter as an arrowed popover over
// whatever presented it, a look no other client has.
//
// Data-driven so any screen can adopt it: a title, an optional body, the
// actions in reading order and the no-answer path. A prompt with an entry in
// `prompts.json` is built from its `PromptCopy` (`GlassAlert(prompt:)`), so
// its words, roles, order and focus come from the contract. A tap on the scrim is that
// path (`onDismiss`), so is the VoiceOver escape gesture; a hardware Esc takes
// the action marked `isCancel`; no close ✕, as on every phone alert (EXP-687).
//
// Not mirrored (an iOS deviation from `prompts.json`): the busy state. The
// presenter closes the card in the same update as the answer's tap, so there
// is no row to disable while an answer runs.

/// One button of a `GlassAlert`.
public struct GlassAlertAction: Identifiable {
    public enum Role: Sendable {
        /// The loud `primary` pill (web `size="sm"` `default`); the default
        /// answer (Return on a hardware keyboard) unless `isDefault` says
        /// otherwise.
        case primary
        /// The glass hairline pill (web `size="sm"` `outline`).
        case outline
        /// The glass pill with the label + hairline in `destructive` (web
        /// `outline` + `text-destructive`): a destructive CONFIRM, never a
        /// solid red block.
        case destructive
        /// Text only in `destructive`, no fill or hairline, set apart on the
        /// row's LEADING edge (web `ghost` + `text-destructive`).
        case quietDestructive
    }

    public let id: String
    public let label: String
    public let role: Role
    /// A disabled action stays in place, dimmed (the pill's disabled paint).
    public let enabled: Bool
    /// The default answer: it takes Return on a hardware keyboard (the
    /// prompt's `focus`). Defaults to the `.primary` role; never a destructive
    /// answer; a caller moves it off a DISABLED primary onto another answer
    /// (EXP-1212: Create issue disabled, then Save draft).
    public let isDefault: Bool
    /// The no-answer pill (the contract's `cancel` role): it takes Esc on a
    /// hardware keyboard and is announced as a cancel button. Its paint stays
    /// the `role`'s; a contract prompt marks its `cancel` action itself.
    public let isCancel: Bool
    public let handler: () -> Void

    public init(
        _ label: String,
        role: Role = .outline,
        enabled: Bool = true,
        isDefault: Bool? = nil,
        isCancel: Bool = false,
        id: String? = nil,
        handler: @escaping () -> Void
    ) {
        self.id = id ?? label
        self.label = label
        self.role = role
        self.enabled = enabled
        self.isDefault = (isDefault ?? (role == .primary))
            && role != .destructive && role != .quietDestructive
        self.isCancel = isCancel
        self.handler = handler
    }
}

/// Geometry of the card, byte-matched to the web's phone arm.
public enum GlassAlertMetrics {
    /// `w-[calc(100%-3rem)]`: 24pt either side.
    public static let screenInset: CGFloat = DesignTokens.Spacing.xl
    /// `max-w-sm`.
    public static let maxWidth: CGFloat = 384
    /// `max-sm:p-5`.
    public static let padding: CGFloat = 20
    /// `rounded-2xl`.
    public static let cornerRadius: CGFloat = DesignTokens.Radius.xl
    /// `gap-4` between the title and the button row.
    public static let sectionGap: CGFloat = DesignTokens.Spacing.lg
    /// `gap-2`: title↔body and between buttons.
    public static let itemGap: CGFloat = DesignTokens.Spacing.sm
    /// The compact button: `GlassPill` `.md` (web `size="sm"`, 32pt).
    public static let buttonHeight: CGFloat = GlassPillTokens.heightMd
    /// Each button's hit area grows to 44pt tall around the 32pt pill.
    public static let minTapHeight: CGFloat = 44
    /// The scrim's `bg-black/60`.
    public static let scrimOpacity: Double = 0.6
}

/// Pure ordering of a `GlassAlert`'s row, unit-tested (`GlassAlertLayoutTests`).
public enum GlassAlertLayout {
    /// The quiet destructive answers, set apart on the LEADING edge.
    public static func leading(_ actions: [GlassAlertAction]) -> [GlassAlertAction] {
        actions.filter { $0.role == .quietDestructive }
    }

    /// Every other answer, packed on the trailing edge in reading order.
    public static func trailing(_ actions: [GlassAlertAction]) -> [GlassAlertAction] {
        actions.filter { $0.role != .quietDestructive }
    }

    /// The stacked fallback (the row does not fit): one pill per line in
    /// REVERSE display order, so the default (last in reading order) sits on
    /// top and Cancel at the bottom, the quiet one last of all.
    public static func stacked(_ actions: [GlassAlertAction]) -> [GlassAlertAction] {
        trailing(actions).reversed() + leading(actions)
    }

    /// The answer Return takes (the prompt's `focus`), never a destructive
    /// one; none when nothing claims it.
    public static func defaultActionId(_ actions: [GlassAlertAction]) -> String? {
        actions.first { $0.isDefault }?.id
    }
}

/// The card itself, with no presentation: what `.glassAlert` hosts, and what
/// a preview or a gallery draws in place. `content` (EXP-1215) is the slot
/// between the title and the row for a dialog that needs more than text (the
/// blocked-start dependency graph).
public struct GlassAlert<Content: View>: View {
    let title: String
    let message: String?
    let actions: [GlassAlertAction]
    let content: Content

    public init(
        title: String,
        message: String? = nil,
        actions: [GlassAlertAction],
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.message = message
        self.actions = actions
        self.content = content()
    }

    /// The same card with every action's handler rewrapped (the presenter
    /// closes the alert before an answer runs).
    func mapActions(_ transform: (GlassAlertAction) -> GlassAlertAction) -> GlassAlert {
        GlassAlert(title: title, message: message, actions: actions.map(transform)) { content }
    }

    private var leading: [GlassAlertAction] { GlassAlertLayout.leading(actions) }
    private var trailing: [GlassAlertAction] { GlassAlertLayout.trailing(actions) }

    public var body: some View {
        VStack(alignment: .leading, spacing: GlassAlertMetrics.sectionGap) {
            VStack(alignment: .leading, spacing: GlassAlertMetrics.itemGap) {
                Text(title)
                    .font(.system(
                        size: DesignTokens.Typography.Size.lg,
                        weight: DesignTokens.Typography.Weight.semibold
                    ))
                    .foregroundStyle(DesignTokens.Palette.foreground)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("glass-alert-title")
                if let message {
                    Text(message)
                        .font(.system(size: 15))
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        .accessibilityIdentifier("glass-alert-message")
                }
            }
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)

            if Content.self != EmptyView.self {
                content
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("glass-alert-content")
            }

            ViewThatFits(in: .horizontal) {
                // ONE row: quiet destructive leading, the rest trailing.
                // No Spacer without a leading answer: as an HStack child it
                // would cost one more `itemGap` and push a row that fits
                // into the stack.
                HStack(spacing: GlassAlertMetrics.itemGap) {
                    if !leading.isEmpty {
                        ForEach(leading) { GlassAlertButton(action: $0) }
                        Spacer(minLength: GlassAlertMetrics.itemGap)
                    }
                    ForEach(trailing) { GlassAlertButton(action: $0) }
                }
                .frame(maxWidth: .infinity, alignment: .trailing)
                // Stacked fallback: natural-width pills, trailing-aligned,
                // the default on top, Cancel under it, the quiet one last.
                VStack(alignment: .trailing, spacing: 0) {
                    ForEach(GlassAlertLayout.stacked(actions)) { GlassAlertButton(action: $0) }
                }
                .frame(maxWidth: .infinity, alignment: .trailing)
            }
            // The 44pt hit areas overhang the 32pt pills; keep the VISUAL
            // gap at `sectionGap` and the card's bottom padding at `padding`.
            .padding(.vertical, -(GlassAlertMetrics.minTapHeight - GlassAlertMetrics.buttonHeight) / 2)
        }
        .padding(GlassAlertMetrics.padding)
        .background(
            GlassTokens.backgroundBottom,
            in: RoundedRectangle(cornerRadius: GlassAlertMetrics.cornerRadius)
        )
        .overlay(
            RoundedRectangle(cornerRadius: GlassAlertMetrics.cornerRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline * 2)
        )
        .frame(maxWidth: GlassAlertMetrics.maxWidth)
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(.isModal)
        .accessibilityIdentifier("glass-alert")
    }
}

public extension GlassAlert where Content == EmptyView {
    /// A text-only alert: the question, an optional body, the row.
    init(title: String, message: String? = nil, actions: [GlassAlertAction]) {
        self.init(title: title, message: message, actions: actions) { EmptyView() }
    }
}

/// One compact alert button: the app's `GlassPill` `.md` paint (primary,
/// glass, or glass toned `destructive`), or bare destructive text for the
/// quiet answer. The pill is drawn readonly inside OUR Button so the hit area
/// can grow to 44pt tall without the pill growing.
private struct GlassAlertButton: View {
    let action: GlassAlertAction

    var body: some View {
        Button(role: buttonRole, action: action.handler) {
            label
                .frame(minHeight: GlassAlertMetrics.minTapHeight)
                .contentShape(Rectangle())
        }
        .buttonStyle(.glassPillPrimary)
        .disabled(!action.enabled)
        .modifier(KeyboardAction(
            isDefault: action.isDefault && action.enabled,
            isCancel: action.isCancel && action.enabled
        ))
        .accessibilityIdentifier("glass-alert-\(action.id)")
    }

    /// The system role VoiceOver announces: destructive for both destructive
    /// paints, cancel for the marked no-answer pill. The paint is ours, so the
    /// role never changes the look.
    private var buttonRole: ButtonRole? {
        switch action.role {
        case .destructive, .quietDestructive: .destructive
        case .primary, .outline: action.isCancel ? .cancel : nil
        }
    }

    @ViewBuilder
    private var label: some View {
        switch action.role {
        case .primary:
            GlassPill(action.label, size: .md, primary: true, enabled: action.enabled)
        case .outline:
            GlassPill(action.label, size: .md, enabled: action.enabled)
        case .destructive:
            GlassPill(
                action.label,
                size: .md,
                tint: DesignTokens.Palette.destructive,
                enabled: action.enabled
            )
        case .quietDestructive:
            // Text only; its padding is a hit margin, pulled back so the
            // label lines up with the title's leading edge.
            Text(action.label)
                .font(GlassPillSize.md.font)
                .lineLimit(1)
                .foregroundStyle(DesignTokens.Palette.destructive.opacity(action.enabled ? 1 : 0.5))
                .padding(.horizontal, GlassPillTokens.horizontalPaddingMd)
                .frame(height: GlassAlertMetrics.buttonHeight)
                .padding(.leading, -GlassPillTokens.horizontalPaddingMd)
        }
    }

    /// The default answer takes Return on a hardware keyboard, the cancel
    /// answer Esc.
    private struct KeyboardAction: ViewModifier {
        let isDefault: Bool
        let isCancel: Bool

        func body(content: Content) -> some View {
            if isDefault {
                content.keyboardShortcut(.defaultAction)
            } else if isCancel {
                content.keyboardShortcut(.cancelAction)
            } else {
                content
            }
        }
    }
}

public extension View {
    /// Presents a `GlassAlert` ABOVE everything, navigation bar and keyboard
    /// included. An action's tap closes the alert and runs its handler in
    /// the same update; a tap on the scrim closes it and runs `onDismiss`
    /// (= dismissed WITHOUT an answer).
    func glassAlert(
        isPresented: Binding<Bool>,
        title: String,
        message: String? = nil,
        actions: [GlassAlertAction],
        onDismiss: (() -> Void)? = nil
    ) -> some View {
        glassAlert(isPresented: isPresented, onDismiss: onDismiss) {
            GlassAlert(title: title, message: message, actions: actions)
        }
    }

    /// The same, with the card built by the caller (a `content` slot).
    func glassAlert<C: View>(
        isPresented: Binding<Bool>,
        onDismiss: (() -> Void)? = nil,
        alert: @escaping () -> GlassAlert<C>
    ) -> some View {
        modifier(GlassAlertPresenter(
            isPresented: isPresented,
            onDismiss: onDismiss,
            alert: { alert() }
        ))
    }

    /// Item-bound (EXP-1215): shown while `item` is non-nil, built from it;
    /// an answer or a scrim tap sets it back to nil first. Handlers capture
    /// the item they were built with, never re-read the binding.
    func glassAlert<Item, C: View>(
        item: Binding<Item?>,
        onDismiss: (() -> Void)? = nil,
        alert: @escaping (Item) -> GlassAlert<C>
    ) -> some View {
        modifier(GlassAlertPresenter(
            isPresented: Binding(
                get: { item.wrappedValue != nil },
                set: { if !$0 { item.wrappedValue = nil } }
            ),
            onDismiss: onDismiss,
            alert: { item.wrappedValue.map(alert) }
        ))
    }
}

/// Hosted like `GlassMenu`'s popup: a clear `.fullScreenCover` (the only
/// SwiftUI surface that paints over a navigation bar and the keyboard), whose
/// present and dismiss run animation-free so the system slide never shows;
/// the scrim and card supply their own cross-dissolve.
private struct GlassAlertPresenter<C: View>: ViewModifier {
    @Binding var isPresented: Bool
    let onDismiss: (() -> Void)?
    let alert: () -> GlassAlert<C>?

    private func setPresented(_ value: Bool) {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { isPresented = value }
    }

    private var coverBinding: Binding<Bool> {
        Binding(get: { isPresented }, set: { setPresented($0) })
    }

    func body(content: Content) -> some View {
        content
            .onChange(of: isPresented) { _, presented in
                // The keyboard must never push or sit on the alert.
                if presented {
                    UIApplication.shared.sendAction(
                        #selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil
                    )
                }
            }
            .fullScreenCover(isPresented: coverBinding) {
                if let card = alert() {
                    GlassAlertHost(
                        alert: card.mapActions { action in
                            GlassAlertAction(
                                action.label,
                                role: action.role,
                                enabled: action.enabled,
                                isDefault: action.isDefault,
                                isCancel: action.isCancel,
                                id: action.id
                            ) {
                                setPresented(false)
                                action.handler()
                            }
                        },
                        dismiss: {
                            setPresented(false)
                            onDismiss?()
                        }
                    )
                    .presentationBackground(.clear)
                } else {
                    Color.clear.presentationBackground(.clear)
                }
            }
            // A cover presented WITHOUT animation leaves the transaction to
            // the caller: flip the caller's own writes animation-free too.
            .transaction(value: isPresented) { $0.disablesAnimations = true }
    }
}

/// The full-window layer: the blurred, dimmed scrim (tap = dismiss) and the
/// centred card, fading + zooming in from 95% like the web's `zoom-in-95`.
private struct GlassAlertHost<C: View>: View {
    let alert: GlassAlert<C>
    let dismiss: () -> Void

    @Environment(\.motion) private var motion
    @State private var appeared = false

    var body: some View {
        ZStack {
            ZStack {
                Rectangle().fill(.ultraThinMaterial)
                Color.black.opacity(GlassAlertMetrics.scrimOpacity)
            }
            .environment(\.colorScheme, .dark)
            .contentShape(Rectangle())
            .onTapGesture { dismiss() }
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel("Dismiss")
            .accessibilityIdentifier("glass-alert-scrim")

            alert
                .scaleEffect(appeared ? 1 : 0.95)
                .padding(.horizontal, GlassAlertMetrics.screenInset)
        }
        .opacity(appeared ? 1 : 0)
        .ignoresSafeArea()
        // The VoiceOver escape gesture (two-finger Z) = the no-answer path,
        // like the scrim tap.
        .accessibilityAction(.escape) { dismiss() }
        .onAppear {
            withAnimation(motion.decelerate()) { appeared = true }
        }
    }
}

// MARK: - Contract prompts (EXP-1215)

public extension GlassAlertAction.Role {
    /// The card paint for a contract role: `cancel` and `default` are the
    /// plain pill.
    init(_ role: PromptRole) {
        switch role {
        case .cancel, .default: self = .outline
        case .primary: self = .primary
        case .destructive: self = .destructive
        case .quietDestructive: self = .quietDestructive
        }
    }
}

public extension GlassAlert where Content == EmptyView {
    /// A prompt from `prompts.json`: its title, body, answers (labels,
    /// roles, display order) and focus come from the filled `PromptCopy`;
    /// the site passes only what each answer DOES, keyed by action id. An
    /// answer with no handler only dismisses (Cancel); `enabled` dims one in
    /// place.
    init(
        prompt: PromptCopy,
        enabled: [String: Bool] = [:],
        handlers: [String: () -> Void]
    ) {
        self.init(
            title: prompt.title,
            message: prompt.body,
            actions: prompt.actions.map { action in
                GlassAlertAction(
                    action.label,
                    role: GlassAlertAction.Role(action.role),
                    enabled: enabled[action.id] ?? true,
                    isDefault: action.id == prompt.focus,
                    isCancel: action.role == .cancel,
                    id: action.id,
                    handler: handlers[action.id] ?? {}
                )
            }
        )
    }
}

// MARK: - Specimens (styleguide `draft-leave-dialog`, EXP-1212)

#Preview("draft-leave-dialog: leave") {
    ZStack {
        GlassTokens.backgroundTop.ignoresSafeArea()
        GlassAlert(
            title: "Save this issue as a draft?",
            actions: [
                GlassAlertAction("Discard", role: .quietDestructive) {},
                GlassAlertAction("Save draft", role: .outline) {},
                GlassAlertAction("Create issue", role: .primary) {},
            ]
        )
        .padding(.horizontal, GlassAlertMetrics.screenInset)
    }
}

#Preview("draft-leave-dialog: leave, no title") {
    ZStack {
        GlassTokens.backgroundTop.ignoresSafeArea()
        GlassAlert(
            title: "Save this issue as a draft?",
            actions: [
                GlassAlertAction("Discard", role: .quietDestructive) {},
                GlassAlertAction("Save draft", role: .outline, isDefault: true) {},
                GlassAlertAction("Create issue", role: .primary, enabled: false, isDefault: false) {},
            ]
        )
        .padding(.horizontal, GlassAlertMetrics.screenInset)
    }
}

#Preview("draft-leave-dialog: discard confirm") {
    ZStack {
        GlassTokens.backgroundTop.ignoresSafeArea()
        GlassAlert(
            title: "Discard this draft and its files?",
            actions: [
                GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true) {},
                GlassAlertAction("Discard", role: .destructive) {},
            ]
        )
        .padding(.horizontal, GlassAlertMetrics.screenInset)
    }
}
