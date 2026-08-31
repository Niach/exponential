import SwiftUI

// Shared glass controls (EXP-604) — the iOS twins of Android's
// ui/components/GlassPillButton.kt, CircleIconButton.kt and GlassTextField.kt,
// which were themselves 1:1 ports of the ad-hoc styling these extract.
// Geometry (paddings, icon sizes, radii, hairlines) is byte-matched across the
// platforms; fills/strokes stay in each platform's glass vocabulary
// (`.glassButton()` / `TextOpacity` here, GlassTokens there). Fonts
// approximate: Android labelMedium ≈ `.caption.weight(.medium)`,
// bodyLarge ≈ `.body.weight(.medium)`.
//
// Absent twins, deliberately: sheets have no close control at all since
// EXP-687 (swipe down, both platforms), and the 52pt `.ultraThinMaterial`
// floating-bar circles are a different chrome class.

// MARK: - Pill

/// The styled content of a glass pill — for the rare hosts that are not a
/// `Button` (NavigationLink labels, tap-gesture rows). Everything else uses
/// `GlassPillButton`.
public struct GlassPillLabel<Leading: View>: View {
    let label: String
    var isActive: Bool = false
    var isOpaque: Bool = false
    var enabled: Bool = true
    /// EXP-678: the ONE geometry knob — a pill sitting beside a taller row
    /// (the steer screen's "Latest changes" chip) matches its height instead
    /// of drawing a short pill against it. Default = the shared 6pt.
    var verticalPadding: CGFloat = 6
    let leading: Leading

    public init(
        _ label: String,
        isActive: Bool = false,
        isOpaque: Bool = false,
        enabled: Bool = true,
        verticalPadding: CGFloat = 6,
        @ViewBuilder leading: () -> Leading
    ) {
        self.label = label
        self.isActive = isActive
        self.isOpaque = isOpaque
        self.enabled = enabled
        self.verticalPadding = verticalPadding
        self.leading = leading()
    }

    public var body: some View {
        HStack(spacing: 6) {
            leading
            Text(label)
                .font(.caption.weight(.medium))
        }
        .foregroundStyle(.white.opacity(enabled ? TextOpacity.primary : TextOpacity.quaternary))
        .padding(.horizontal, 12)
        .padding(.vertical, verticalPadding)
        .glassButton(isActive: isActive, isOpaque: isOpaque)
    }
}

extension GlassPillLabel where Leading == EmptyView {
    public init(
        _ label: String,
        isActive: Bool = false,
        isOpaque: Bool = false,
        enabled: Bool = true,
        verticalPadding: CGFloat = 6
    ) {
        self.init(
            label,
            isActive: isActive,
            isOpaque: isOpaque,
            enabled: enabled,
            verticalPadding: verticalPadding
        ) { EmptyView() }
    }
}

/// The capsule action pill: optional 14pt leading icon, caption/medium label.
public struct GlassPillButton: View {
    let label: String
    var icon: String? = nil
    var isActive: Bool = false
    var isOpaque: Bool = false
    var enabled: Bool = true
    var verticalPadding: CGFloat = 6
    let action: () -> Void

    public init(
        _ label: String,
        icon: String? = nil,
        isActive: Bool = false,
        isOpaque: Bool = false,
        enabled: Bool = true,
        verticalPadding: CGFloat = 6,
        action: @escaping () -> Void
    ) {
        self.label = label
        self.icon = icon
        self.isActive = isActive
        self.isOpaque = isOpaque
        self.enabled = enabled
        self.verticalPadding = verticalPadding
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            GlassPillLabel(
                label,
                isActive: isActive,
                isOpaque: isOpaque,
                enabled: enabled,
                verticalPadding: verticalPadding
            ) {
                if let icon {
                    AppIcon(icon, size: 14)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

// MARK: - Submit

/// The styled content of the full-width submit button — for `Link` hosts
/// (UpdateRequiredView). Everything else uses `GlassSubmitButton`.
///
/// EXP-694 (S1, Android `GlassSubmitButton` parity): a LIVE submit is the solid
/// `primary` fill with `primaryForeground` content and no hairline — the dark
/// white-alpha glass it used to be read as disabled next to the real disabled
/// state. Disabled is unchanged (white .06 + hairline, tertiary label), and the
/// geometry (radius 10, 14pt v-padding) is untouched on both.
public struct GlassSubmitLabel<Icon: View>: View {
    let label: String
    var enabled: Bool = true
    /// Swaps the label for a white ProgressView; geometry unchanged.
    var loading: Bool = false
    let icon: Icon

    public init(
        _ label: String,
        enabled: Bool = true,
        loading: Bool = false,
        @ViewBuilder icon: () -> Icon
    ) {
        self.label = label
        self.enabled = enabled
        self.loading = loading
        self.icon = icon()
    }

    public var body: some View {
        HStack(spacing: 8) {
            if loading {
                ProgressView()
                    .tint(enabled ? DesignTokens.Palette.primaryForeground : Color.white)
            } else {
                icon
                Text(label)
            }
        }
        .font(.body.weight(.medium))
        .foregroundStyle(
            enabled
                ? DesignTokens.Palette.primaryForeground
                : Color.white.opacity(TextOpacity.tertiary)
        )
        .frame(maxWidth: .infinity)
        .padding(.vertical, 14)
        .background(enabled ? DesignTokens.Palette.primary : Color.white.opacity(0.06))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        // A filled button needs no hairline — only the disabled glass does.
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(enabled ? Color.clear : Color.white.opacity(0.1), lineWidth: 0.5)
        )
    }
}

extension GlassSubmitLabel where Icon == EmptyView {
    public init(_ label: String, enabled: Bool = true, loading: Bool = false) {
        self.init(label, enabled: enabled, loading: loading) { EmptyView() }
    }
}

public struct GlassSubmitButton: View {
    let label: String
    var enabled: Bool = true
    var loading: Bool = false
    let action: () -> Void

    public init(_ label: String, enabled: Bool = true, loading: Bool = false, action: @escaping () -> Void) {
        self.label = label
        self.enabled = enabled
        self.loading = loading
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            GlassSubmitLabel(label, enabled: enabled, loading: loading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled || loading)
    }
}

// MARK: - OAuth

/// The full-width provider button: dimmer fill, brighter hairline, pure white
/// content, required icon slot (`EmptyView` for OIDC providers with no mark).
public struct GlassOAuthButton<Icon: View>: View {
    let label: String
    let action: () -> Void
    let icon: Icon

    public init(_ label: String, action: @escaping () -> Void, @ViewBuilder icon: () -> Icon) {
        self.label = label
        self.action = action
        self.icon = icon()
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                icon
                Text(label)
            }
            .font(.body.weight(.medium))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
            .background(Color.white.opacity(0.08))
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Toggle

/// The ONE switch (EXP-643): Android `Switch` parity under the dark M3 scheme
/// — an on switch is a `primaryForeground` (near-black) thumb on a `primary`
/// (near-white) track. The system `.switch` style can only tint the TRACK and
/// always paints a white thumb, so tinting it `primary` gave white-on-white
/// with no contrast. Off is the glass fill + hairline with a muted thumb.
/// Geometry matches UISwitch (51×31, 27pt thumb). Applied app-wide from the
/// root (`.toggleStyle(.glass)`), so `Toggle` call sites stay stock; a hidden
/// label host adds `.fixedSize()` to keep the toggle from claiming row width.
public struct GlassToggleStyle: ToggleStyle {
    @Environment(\.isEnabled) private var isEnabled
    @Environment(\.motion) private var motion

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Button {
            withAnimation(motion.fast) {
                configuration.isOn.toggle()
            }
        } label: {
            HStack(spacing: 8) {
                configuration.label
                Spacer(minLength: 0)
                track(isOn: configuration.isOn)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(isEnabled ? 1 : 0.4)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(configuration.isOn ? "On" : "Off")
    }

    private func track(isOn: Bool) -> some View {
        Capsule()
            .fill(isOn ? DesignTokens.Palette.primary : Color.white.opacity(0.08))
            .overlay(
                Capsule()
                    .stroke(Color.white.opacity(isOn ? 0 : 0.15), lineWidth: 0.5)
            )
            .overlay(alignment: isOn ? .trailing : .leading) {
                Circle()
                    .fill(isOn ? DesignTokens.Palette.primaryForeground : Zinc._400)
                    .frame(width: 27, height: 27)
                    .padding(2)
            }
            .frame(width: 51, height: 31)
    }
}

extension ToggleStyle where Self == GlassToggleStyle {
    public static var glass: GlassToggleStyle { GlassToggleStyle() }
}

// MARK: - Circle icon button

/// A drawn glass circle around a single glyph (Android CircleIconButton:
/// 38pt circle, 20pt glyph).
public struct CircleIconButton: View {
    let icon: String
    let accessibilityLabel: String
    var size: CGFloat = 38
    var glyphSize: CGFloat = 20
    var tint: Color? = nil
    var enabled: Bool = true
    let action: () -> Void

    public init(
        _ icon: String,
        accessibilityLabel: String,
        size: CGFloat = 38,
        glyphSize: CGFloat = 20,
        tint: Color? = nil,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) {
        self.icon = icon
        self.accessibilityLabel = accessibilityLabel
        self.size = size
        self.glyphSize = glyphSize
        self.tint = tint
        self.enabled = enabled
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            AppIcon(icon, size: glyphSize, weight: .medium)
                .foregroundStyle(
                    tint ?? .white.opacity(enabled ? TextOpacity.secondary : TextOpacity.quaternary)
                )
                .frame(width: size, height: size)
                .background(Color.white.opacity(0.06), in: Circle())
                .overlay(
                    Circle()
                        .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
                )
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

/// The one back button for a `topBarLeading` slot on a page that hides the
/// system chevron (EXP-687 made New issue a page) — Android's
/// `TopBarBackButton`, the same circle around the `ui-back` glyph.
public struct TopBarBackButton: View {
    var accessibilityLabel: String = "Back"
    var enabled: Bool = true
    let action: () -> Void

    public init(
        accessibilityLabel: String = "Back",
        enabled: Bool = true,
        action: @escaping () -> Void
    ) {
        self.accessibilityLabel = accessibilityLabel
        self.enabled = enabled
        self.action = action
    }

    public var body: some View {
        CircleIconButton(
            AppIcons.uiBack,
            accessibilityLabel: accessibilityLabel,
            size: 32,
            glyphSize: 17,
            enabled: enabled,
            action: action
        )
    }
}

// MARK: - Text field

/// The ONE glass text input (Android GlassTextField parity): 12pt corners,
/// white .06 fill, hairline that brightens while focused. Behavior modifiers
/// (`.keyboardType`, `.submitLabel`, `.font`, autocapitalization/correction,
/// `.focused`, `.onSubmit`) are applied by the CALLER — they propagate to the
/// inner field. `showsBackground: false` drops the chrome and padding for
/// fields living inside already-chromed rows (Form).
public struct GlassTextField<Leading: View, Trailing: View>: View {
    let placeholder: String
    @Binding var text: String
    var isSecure: Bool = false
    var showsBackground: Bool = true
    var horizontalPadding: CGFloat = 14
    var verticalPadding: CGFloat = 12
    /// Applied directly to the field so XCUITest `textFields[id]` queries keep
    /// resolving (an identifier on the container lands on the wrong element).
    var accessibilityIdentifier: String = ""
    let leading: Leading
    let trailing: Trailing

    @FocusState private var focused: Bool

    public init(
        _ placeholder: String,
        text: Binding<String>,
        isSecure: Bool = false,
        showsBackground: Bool = true,
        horizontalPadding: CGFloat = 14,
        verticalPadding: CGFloat = 12,
        accessibilityIdentifier: String = "",
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.placeholder = placeholder
        self._text = text
        self.isSecure = isSecure
        self.showsBackground = showsBackground
        self.horizontalPadding = horizontalPadding
        self.verticalPadding = verticalPadding
        self.accessibilityIdentifier = accessibilityIdentifier
        self.leading = leading()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: 8) {
            leading
            field
                .accessibilityIdentifier(accessibilityIdentifier)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .focused($focused)
            trailing
        }
        .modifier(Chrome(
            shows: showsBackground,
            horizontalPadding: horizontalPadding,
            verticalPadding: verticalPadding,
            focused: focused
        ))
    }

    @ViewBuilder
    private var field: some View {
        if isSecure {
            // Under -uiTesting (fastlane snapshot), use a PLAIN field: the
            // system "Save Password?" sheet only triggers on secure text
            // entry, appears seconds later at an unbeatable moment, and is
            // hosted outside the app so XCUITest cannot reliably dismiss
            // it (it photobombed the store shots repeatedly). No secure
            // field ⇒ no sheet. Real users always get the SecureField.
            if ProcessInfo.processInfo.arguments.contains("-uiTesting") {
                TextField(placeholder, text: $text)
            } else {
                SecureField(placeholder, text: $text)
            }
        } else {
            TextField(placeholder, text: $text)
        }
    }

    private struct Chrome: ViewModifier {
        let shows: Bool
        let horizontalPadding: CGFloat
        let verticalPadding: CGFloat
        let focused: Bool

        func body(content: Content) -> some View {
            if shows {
                content
                    .padding(.horizontal, horizontalPadding)
                    .padding(.vertical, verticalPadding)
                    .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12)
                            .stroke(Color.white.opacity(focused ? 0.2 : 0.1), lineWidth: 0.5)
                    )
            } else {
                content
            }
        }
    }
}

extension GlassTextField where Leading == EmptyView, Trailing == EmptyView {
    public init(
        _ placeholder: String,
        text: Binding<String>,
        isSecure: Bool = false,
        showsBackground: Bool = true,
        horizontalPadding: CGFloat = 14,
        verticalPadding: CGFloat = 12,
        accessibilityIdentifier: String = ""
    ) {
        self.init(
            placeholder,
            text: text,
            isSecure: isSecure,
            showsBackground: showsBackground,
            horizontalPadding: horizontalPadding,
            verticalPadding: verticalPadding,
            accessibilityIdentifier: accessibilityIdentifier
        ) { EmptyView() } trailing: { EmptyView() }
    }
}

// MARK: - Search field

/// Inline search field for the searchable sheets — deliberately NOT system
/// `.searchable` (iOS 26 renders that as a bottom-edge glass bar; see
/// DuplicatePickerSheet, whose styling this extracts). Layout margins are the
/// caller's. `showsBackground: false` for bare Form rows (StartCodingSheet).
public struct GlassSheetSearchField: View {
    let placeholder: String
    @Binding var text: String
    var showsBackground: Bool = true
    var accessibilityIdentifier: String = ""

    public init(
        placeholder: String,
        text: Binding<String>,
        showsBackground: Bool = true,
        accessibilityIdentifier: String = ""
    ) {
        self.placeholder = placeholder
        self._text = text
        self.showsBackground = showsBackground
        self.accessibilityIdentifier = accessibilityIdentifier
    }

    public var body: some View {
        GlassTextField(
            placeholder,
            text: $text,
            showsBackground: showsBackground,
            horizontalPadding: 12,
            verticalPadding: 9,
            accessibilityIdentifier: accessibilityIdentifier
        ) {
            AppIcon(AppIcons.navSearch, size: AppIcon.Size.small)
                .foregroundStyle(.secondary)
        } trailing: {
            if !text.isEmpty {
                Button {
                    text = ""
                } label: {
                    AppIcon(AppIcons.uiClear, size: AppIcon.Size.small)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .submitLabel(.search)
    }
}
