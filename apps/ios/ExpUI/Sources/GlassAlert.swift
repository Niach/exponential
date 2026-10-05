import SwiftUI
import UIKit

// The app's OWN alert (EXP-1212): the iOS twin of the web phone layout's
// centred alert card (`packages/ui` `MOBILE_ALERT` + `AlertDialogFooter`) —
// a dimmed, blurred scrim, a centred glass card, a left-aligned title (the
// question; an optional muted body) and ONE compact row of `GlassPill` `.md`
// buttons: a quiet destructive text answer set apart on the leading edge, the
// rest trailing (Thunderbird's save prompt). The row stacks only when it does
// not fit (large Dynamic Type). Never the system `.alert` /
// `.confirmationDialog`: iOS 26+ draws the latter as an arrowed popover over
// whatever presented it, a look no other client has.
//
// Data-driven so any screen can adopt it: a title, an optional body, the
// actions in reading order and the no-answer path. A tap on the scrim is that
// path (`onDismiss`); no close ✕, as on every phone alert (EXP-687).

/// One button of a `GlassAlert`.
public struct GlassAlertAction: Identifiable {
    public enum Role: Sendable {
        /// The loud `primary` pill (web `size="sm"` `default`); the default
        /// answer: it takes Return on a hardware keyboard.
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
    public let handler: () -> Void

    public init(
        _ label: String,
        role: Role = .outline,
        enabled: Bool = true,
        id: String? = nil,
        handler: @escaping () -> Void
    ) {
        self.id = id ?? label
        self.label = label
        self.role = role
        self.enabled = enabled
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

/// The card itself, with no presentation: what `.glassAlert` hosts, and what
/// a preview or a gallery draws in place.
public struct GlassAlert: View {
    let title: String
    let message: String?
    let actions: [GlassAlertAction]

    public init(title: String, message: String? = nil, actions: [GlassAlertAction]) {
        self.title = title
        self.message = message
        self.actions = actions
    }

    private var leadingActions: [GlassAlertAction] {
        actions.filter { $0.role == .quietDestructive }
    }

    private var trailingActions: [GlassAlertAction] {
        actions.filter { $0.role != .quietDestructive }
    }

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
                if let message {
                    Text(message)
                        .font(.system(size: 15))
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                }
            }
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)

            ViewThatFits(in: .horizontal) {
                // ONE row: quiet destructive leading, the rest trailing.
                HStack(spacing: GlassAlertMetrics.itemGap) {
                    ForEach(leadingActions) { GlassAlertButton(action: $0) }
                    Spacer(minLength: GlassAlertMetrics.itemGap)
                    ForEach(trailingActions) { GlassAlertButton(action: $0) }
                }
                // Stacked fallback: the trailing answers, default last in
                // reading order = on top, then the quiet one.
                VStack(alignment: .trailing, spacing: 0) {
                    ForEach(trailingActions.reversed()) { GlassAlertButton(action: $0) }
                    ForEach(leadingActions) { GlassAlertButton(action: $0) }
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
    }
}

/// One compact alert button: the app's `GlassPill` `.md` paint (primary,
/// glass, or glass toned `destructive`), or bare destructive text for the
/// quiet answer. The pill is drawn readonly inside OUR Button so the hit area
/// can grow to 44pt tall without the pill growing.
private struct GlassAlertButton: View {
    let action: GlassAlertAction

    var body: some View {
        Button(action: action.handler) {
            label
                .frame(minHeight: GlassAlertMetrics.minTapHeight)
                .contentShape(Rectangle())
        }
        .buttonStyle(.glassPillPrimary)
        .disabled(!action.enabled)
        .modifier(DefaultAction(isDefault: action.role == .primary))
        .accessibilityIdentifier("glass-alert-\(action.id)")
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

    /// The primary answer takes Return on a hardware keyboard.
    private struct DefaultAction: ViewModifier {
        let isDefault: Bool

        func body(content: Content) -> some View {
            if isDefault {
                content.keyboardShortcut(.defaultAction)
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
        modifier(GlassAlertPresenter(
            isPresented: isPresented,
            title: title,
            message: message,
            actions: actions,
            onDismiss: onDismiss
        ))
    }
}

/// Hosted like `GlassMenu`'s popup: a clear `.fullScreenCover` (the only
/// SwiftUI surface that paints over a navigation bar and the keyboard), whose
/// present and dismiss run animation-free so the system slide never shows;
/// the scrim and card supply their own cross-dissolve.
private struct GlassAlertPresenter: ViewModifier {
    @Binding var isPresented: Bool
    let title: String
    let message: String?
    let actions: [GlassAlertAction]
    let onDismiss: (() -> Void)?

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
                GlassAlertHost(
                    alert: GlassAlert(
                        title: title,
                        message: message,
                        actions: actions.map { action in
                            GlassAlertAction(
                                action.label,
                                role: action.role,
                                enabled: action.enabled,
                                id: action.id
                            ) {
                                setPresented(false)
                                action.handler()
                            }
                        }
                    ),
                    dismiss: {
                        setPresented(false)
                        onDismiss?()
                    }
                )
                .presentationBackground(.clear)
            }
            // A cover presented WITHOUT animation leaves the transaction to
            // the caller: flip the caller's own writes animation-free too.
            .transaction(value: isPresented) { $0.disablesAnimations = true }
    }
}

/// The full-window layer: the blurred, dimmed scrim (tap = dismiss) and the
/// centred card, fading + zooming in from 95% like the web's `zoom-in-95`.
private struct GlassAlertHost: View {
    let alert: GlassAlert
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
        .onAppear {
            withAnimation(motion.decelerate()) { appeared = true }
        }
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
                GlassAlertAction("Create issue", role: .outline) {},
                GlassAlertAction("Save draft", role: .primary) {},
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
                GlassAlertAction("Cancel", role: .outline) {},
                GlassAlertAction("Discard", role: .destructive) {},
            ]
        )
        .padding(.horizontal, GlassAlertMetrics.screenInset)
    }
}
