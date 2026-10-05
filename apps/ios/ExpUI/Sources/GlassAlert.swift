import SwiftUI
import UIKit

// The app's OWN alert (EXP-1212): the iOS twin of the web phone layout's
// centred alert card (`packages/ui` `MOBILE_ALERT` + `AlertDialogFooter`) —
// a dimmed, blurred scrim, a centred glass card, a left-aligned title and
// muted body, then full-width stacked buttons. Never the system `.alert` /
// `.confirmationDialog`: iOS 26+ draws the latter as an arrowed popover over
// whatever presented it, a look no other client has.
//
// Data-driven so any screen can adopt it: a title, a body, an ORDERED list of
// actions (top to bottom) and the no-answer path. A tap on the scrim is that
// path (`onDismiss`); no close ✕, as on every phone alert (EXP-687).

/// One button of a `GlassAlert`.
public struct GlassAlertAction: Identifiable {
    public enum Role: Sendable {
        /// The light `primary` fill with dark text (web `default`).
        case primary
        /// A hairline-outlined glass button (web `outline`).
        case outline
        /// The solid `destructive` fill with white text (web `destructive`).
        case destructive
    }

    public let id: String
    public let label: String
    public let role: Role
    /// A disabled action stays in place, dimmed (web `disabled:opacity-50`).
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
    /// `gap-4` between the header and the buttons.
    public static let sectionGap: CGFloat = DesignTokens.Spacing.lg
    /// `gap-2`: title↔body and between buttons.
    public static let itemGap: CGFloat = DesignTokens.Spacing.sm
    /// `h-10`, the phone dialog footer's button height.
    public static let buttonHeight: CGFloat = 40
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

            VStack(spacing: GlassAlertMetrics.itemGap) {
                ForEach(actions) { action in
                    GlassAlertButton(action: action)
                }
            }
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

/// One full-width alert button. The fills are the web button variants', read
/// off the same tokens `GlassSubmitLabel` (primary) and the outlined glass
/// buttons use.
private struct GlassAlertButton: View {
    let action: GlassAlertAction

    var body: some View {
        Button(action: action.handler) {
            Text(action.label)
                .font(.system(size: 15, weight: DesignTokens.Typography.Weight.medium))
                .foregroundStyle(foreground)
                .frame(maxWidth: .infinity)
                .frame(height: GlassAlertMetrics.buttonHeight)
                .background(fill, in: shape)
                .overlay(shape.stroke(stroke, lineWidth: GlassTokens.hairline * 2))
                .contentShape(shape)
        }
        .buttonStyle(GlassAlertPressStyle())
        .disabled(!action.enabled)
        .opacity(action.enabled ? 1 : 0.5)
        .accessibilityIdentifier("glass-alert-\(action.id)")
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
    }

    private var foreground: Color {
        switch action.role {
        case .primary: DesignTokens.Palette.primaryForeground
        case .outline: DesignTokens.Palette.foreground
        case .destructive: .white
        }
    }

    private var fill: Color {
        switch action.role {
        case .primary: DesignTokens.Palette.primary
        case .outline: DesignTokens.Palette.input.opacity(0.3)
        case .destructive: DesignTokens.Palette.destructive
        }
    }

    private var stroke: Color {
        action.role == .outline ? DesignTokens.Palette.input : .clear
    }
}

/// Press feedback: the web's `hover:bg-*/90`, as a slight dim.
private struct GlassAlertPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.opacity(configuration.isPressed ? 0.85 : 1)
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
            title: "This issue is still a draft",
            message: "Create it now, keep it as a draft or discard it.",
            actions: [
                GlassAlertAction("Create", role: .primary) {},
                GlassAlertAction("Keep as draft", role: .outline) {},
                GlassAlertAction("Discard", role: .destructive) {},
            ]
        )
        .padding(.horizontal, GlassAlertMetrics.screenInset)
    }
}

#Preview("draft-leave-dialog: discard confirm") {
    ZStack {
        GlassTokens.backgroundTop.ignoresSafeArea()
        GlassAlert(
            title: "Discard draft?",
            message: "This draft and its files will be deleted.",
            actions: [
                GlassAlertAction("Discard", role: .destructive) {},
                GlassAlertAction("Cancel", role: .outline) {},
            ]
        )
        .padding(.horizontal, GlassAlertMetrics.screenInset)
    }
}
