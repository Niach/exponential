import SwiftUI

// Shared glass controls (EXP-604) — the iOS twins of Android's
// ui/components/CircleIconButton.kt and GlassTextField.kt, which were
// themselves 1:1 ports of the ad-hoc styling these extract.
// Geometry (paddings, icon sizes, radii, hairlines) is byte-matched across the
// platforms, and since EXP-698 so are the fills and strokes: both sides read
// `GlassTokens`, which is nothing but `DesignTokens` reads. Fonts approximate:
// Android labelMedium ≈ `.caption.weight(.medium)`, bodyLarge ≈
// `.body.weight(.medium)`.
//
// The pill vocabulary moved OUT of this file in EXP-698 round 2: the three
// types that used to live here (`GlassPillLabel`, `GlassPillButton`,
// `GlassChip`) collapsed into the one `GlassPill` in GlassPill.swift, and the
// three hand-rolled composer cards into `GlassComposer`.
//
// Absent twins, deliberately: sheets have no close control at all since
// EXP-687 (swipe down, both platforms), and the 52pt `.ultraThinMaterial`
// floating-bar circles are a different chrome class.

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
        .background(enabled ? DesignTokens.Palette.primary : GlassTokens.fillCard)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
        // A filled button needs no hairline — only the disabled glass does.
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                .stroke(
                    enabled ? Color.clear : GlassTokens.strokeCard,
                    lineWidth: GlassTokens.hairline
                )
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
            .background(GlassTokens.fillCard)
            .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                    .stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
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
            .fill(isOn ? DesignTokens.Palette.primary : GlassTokens.fillCard)
            .overlay(
                Capsule()
                    .stroke(
                        isOn ? Color.clear : GlassTokens.strokeCard,
                        lineWidth: GlassTokens.hairline
                    )
            )
            .overlay(alignment: isOn ? .trailing : .leading) {
                Circle()
                    .fill(
                        isOn
                            ? DesignTokens.Palette.primaryForeground
                            : DesignTokens.Palette.mutedForeground
                    )
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

/// A drawn glass circle around a single glyph — the ONE chrome for a trailing
/// action, in a list row or a toolbar alike (EXP-698). `controlMd` (32pt) with
/// a 17pt glyph at 70 % white is the default on both mobile clients (Android
/// matches 17dp); web and desktop use a 16px glyph in the same 32px circle.
/// The explicit sizes that survive here are the deliberately smaller in-row
/// ones.
public struct CircleIconButton: View {
    let icon: String
    let accessibilityLabel: String
    var size: CGFloat = GlassTokens.controlSize
    var glyphSize: CGFloat = AppIcon.Size.medium
    var tint: Color? = nil
    var enabled: Bool = true
    let action: () -> Void

    public init(
        _ icon: String,
        accessibilityLabel: String,
        size: CGFloat = GlassTokens.controlSize,
        glyphSize: CGFloat = AppIcon.Size.medium,
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
            CircleIconLabel(
                icon,
                size: size,
                glyphSize: glyphSize,
                tint: tint,
                enabled: enabled
            )
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

/// The styled content of a `CircleIconButton` — for the hosts that are not a
/// `Button` (a `GlassMenu` label, which owns its own tap handling). Everything
/// else uses `CircleIconButton`.
public struct CircleIconLabel: View {
    let icon: String
    var size: CGFloat = GlassTokens.controlSize
    var glyphSize: CGFloat = AppIcon.Size.medium
    var tint: Color? = nil
    var enabled: Bool = true

    public init(
        _ icon: String,
        size: CGFloat = GlassTokens.controlSize,
        glyphSize: CGFloat = AppIcon.Size.medium,
        tint: Color? = nil,
        enabled: Bool = true
    ) {
        self.icon = icon
        self.size = size
        self.glyphSize = glyphSize
        self.tint = tint
        self.enabled = enabled
    }

    public var body: some View {
        AppIcon(icon, size: glyphSize, weight: .medium)
            .foregroundStyle(
                tint ?? .white.opacity(enabled ? TextOpacity.secondary : TextOpacity.quaternary)
            )
            .frame(width: size, height: size)
            .background(GlassTokens.fillCard, in: Circle())
            .overlay(
                Circle()
                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
            .contentShape(Circle())
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
            enabled: enabled,
            action: action
        )
    }
}

// MARK: - Text field

/// The ONE glass text input (Android GlassTextField parity): `fieldRadius`
/// corners, `fillCard`, a hairline that brightens to `strokeActive` while
/// focused. Behavior modifiers
/// (`.keyboardType`, `.submitLabel`, `.font`, autocapitalization/correction,
/// `.focused`, `.onSubmit`) are applied by the CALLER — they propagate to the
/// inner field, which is why there is no `font:` parameter: a prompt box keeps
/// its `.footnote.monospaced()` with a plain modifier.
///
/// `bordered: false` drops the chrome and padding for fields living inside an
/// already-chromed row (a `GlassSection` row, a `Form`, a `GlassComposer`).
///
/// `lines:` makes it a TEXT AREA (EXP-698 round 2): the field grows on the
/// vertical axis within that many lines. It replaces the nine hand-rolled
/// `TextField(axis: .vertical)` boxes that each drew their own fill, radius and
/// padding — the multi-line input is the same control as the single-line one.
public struct GlassTextField<Leading: View, Trailing: View>: View {
    let placeholder: String
    @Binding var text: String
    var isSecure: Bool = false
    /// The growth range of a multi-line field; nil = a single-line field.
    var lines: ClosedRange<Int>? = nil
    var bordered: Bool = true
    var horizontalPadding: CGFloat = 12
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
        lines: ClosedRange<Int>? = nil,
        bordered: Bool = true,
        horizontalPadding: CGFloat = 12,
        verticalPadding: CGFloat = 12,
        accessibilityIdentifier: String = "",
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.placeholder = placeholder
        self._text = text
        self.isSecure = isSecure
        self.lines = lines
        self.bordered = bordered
        self.horizontalPadding = horizontalPadding
        self.verticalPadding = verticalPadding
        self.accessibilityIdentifier = accessibilityIdentifier
        self.leading = leading()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(alignment: lines == nil ? .center : .top, spacing: 8) {
            leading
            field
                .accessibilityIdentifier(accessibilityIdentifier)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .focused($focused)
            trailing
        }
        .modifier(Chrome(
            shows: bordered,
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
        } else if let lines {
            TextField(placeholder, text: $text, axis: .vertical)
                .lineLimit(lines)
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
                    .background(
                        GlassTokens.fillCard,
                        in: RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                            .stroke(
                                focused ? GlassTokens.strokeActive : GlassTokens.strokeCard,
                                lineWidth: GlassTokens.hairline
                            )
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
        lines: ClosedRange<Int>? = nil,
        bordered: Bool = true,
        horizontalPadding: CGFloat = 12,
        verticalPadding: CGFloat = 12,
        accessibilityIdentifier: String = ""
    ) {
        self.init(
            placeholder,
            text: text,
            isSecure: isSecure,
            lines: lines,
            bordered: bordered,
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
/// caller's. `bordered: false` for bare Form rows (StartCodingSheet).
public struct GlassSheetSearchField: View {
    let placeholder: String
    @Binding var text: String
    var bordered: Bool = true
    var accessibilityIdentifier: String = ""

    public init(
        placeholder: String,
        text: Binding<String>,
        bordered: Bool = true,
        accessibilityIdentifier: String = ""
    ) {
        self.placeholder = placeholder
        self._text = text
        self.bordered = bordered
        self.accessibilityIdentifier = accessibilityIdentifier
    }

    public var body: some View {
        GlassTextField(
            placeholder,
            text: $text,
            bordered: bordered,
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
