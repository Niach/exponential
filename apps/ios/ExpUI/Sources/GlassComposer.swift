import SwiftUI

// The ONE composer card (EXP-698 round 2). iOS grew three of them — the issue
// comment composer, the steer composer and the support reply box — and they
// had drifted onto three different chromes: one `opaqueCardFill` at radius 20
// with `strokeStrong`, one `.ultraThinMaterial` at radius 20 with a hand-typed
// `white.opacity(0.12)` hairline, and one `white.opacity(0.06)` at radius 18
// inside an `.ultraThinMaterial` bar. All three carried the same drop shadow,
// the one thing the flat glass language does not have.
//
// `GlassComposer` is that card: `Radius.xl` corners, `fillCard` + the
// `strokeCard` hairline, no material and no shadow. `isOpaque` swaps in
// `opaqueCardFill` + `strokeStrong` for a composer that FLOATS over scrolling
// content, the same switch `GlassCard` and `GlassPill` carry.
//
// The card owns the chrome and the tool row; the slots keep their own inner
// padding, because a `MarkdownEditor` pads itself and a bare `TextField` does
// not. Slot ORDER is leading · field · strip · tools: the attachment strip
// sits BELOW the text and above the action row on every client, so a queued
// image reads as something the message you are typing carries, not as a
// banner over it.

/// The pinned geometry of a composer card, so the three composers cannot drift
/// apart again.
public enum GlassComposerTokens {
    public static let cornerRadius: CGFloat = DesignTokens.Radius.xl
    /// The glyph inside a tool button.
    public static let toolGlyphSize: CGFloat = AppIcon.Size.medium
    /// A tool button's TAP TARGET. Deliberately much larger than the glyph:
    /// these sit in a row of five, a thumb-width apart, above the keyboard —
    /// sizing the frame to the 17pt mark would leave five 17pt targets and a
    /// composer you cannot hit. The glyphs still read as small ghost marks.
    public static let toolHitSize: CGFloat = 36
    /// The submit glyph — a round mark, drawn ghost (no capsule behind it).
    public static let submitSize: CGFloat = 28
    /// Gap between two tool buttons — tight, because `toolHitSize` already
    /// carries the visual pitch.
    public static let toolSpacing: CGFloat = 2
    public static let horizontalPadding: CGFloat = 12
    public static let bottomPadding: CGFloat = 10
}

// MARK: - Tool + submit buttons

/// A ghost glyph button in a composer's tool row (photo, attach, @, #, emoji).
public struct GlassComposerToolButton: View {
    let icon: String
    let accessibilityLabel: String
    var enabled: Bool = true
    let action: () -> Void

    public init(
        _ icon: String,
        accessibilityLabel: String,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) {
        self.icon = icon
        self.accessibilityLabel = accessibilityLabel
        self.enabled = enabled
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            AppIcon(icon, size: GlassComposerTokens.toolGlyphSize)
                .foregroundStyle(
                    .white.opacity(enabled ? TextOpacity.secondary : TextOpacity.quaternary)
                )
                .frame(
                    width: GlassComposerTokens.toolHitSize,
                    height: GlassComposerTokens.toolHitSize
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

/// The round ghost submit at the trailing end of a composer's tool row.
/// `tint` is the LIVE color (white by default; the support composer tints an
/// internal note); disabled drops to the quaternary white.
public struct GlassComposerSubmitButton: View {
    let icon: String
    let accessibilityLabel: String
    var enabled: Bool = true
    var tint: Color = .white
    let action: () -> Void

    public init(
        _ icon: String,
        accessibilityLabel: String,
        enabled: Bool = true,
        tint: Color = .white,
        action: @escaping () -> Void
    ) {
        self.icon = icon
        self.accessibilityLabel = accessibilityLabel
        self.enabled = enabled
        self.tint = tint
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            AppIcon(icon, size: GlassComposerTokens.submitSize)
                .foregroundStyle(enabled ? tint : .white.opacity(TextOpacity.quaternary))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

// MARK: - The card

public struct GlassComposer<
    Leading: View,
    Strip: View,
    Field: View,
    Tools: View,
    Submit: View
>: View {
    /// Lays the opaque card surface beneath the glass tint — for a composer
    /// that floats over a scrolling feed.
    var isOpaque: Bool = false
    let leading: Leading
    let strip: Strip
    let field: Field
    let tools: Tools
    let submit: Submit

    /// Slots are declared in the order they RENDER.
    public init(
        isOpaque: Bool = false,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder field: () -> Field,
        @ViewBuilder strip: () -> Strip,
        @ViewBuilder tools: () -> Tools,
        @ViewBuilder submit: () -> Submit
    ) {
        self.isOpaque = isOpaque
        self.leading = leading()
        self.field = field()
        self.strip = strip()
        self.tools = tools()
        self.submit = submit()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            leading
            field
            strip
            HStack(spacing: GlassComposerTokens.toolSpacing) {
                tools
                Spacer(minLength: 0)
                submit
            }
            .padding(.horizontal, GlassComposerTokens.horizontalPadding)
            .padding(.bottom, GlassComposerTokens.bottomPadding)
        }
        .background(isOpaque ? GlassTokens.opaqueCardFill : GlassTokens.fillCard)
        .clipShape(RoundedRectangle(cornerRadius: GlassComposerTokens.cornerRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassComposerTokens.cornerRadius)
                .stroke(
                    isOpaque ? GlassTokens.strokeStrong : GlassTokens.strokeCard,
                    lineWidth: GlassTokens.hairline
                )
        )
    }
}

// MARK: - Slot-omitting inits

extension GlassComposer where Leading == EmptyView {
    /// No leading row — the issue and steer composers.
    public init(
        isOpaque: Bool = false,
        @ViewBuilder field: () -> Field,
        @ViewBuilder strip: () -> Strip,
        @ViewBuilder tools: () -> Tools,
        @ViewBuilder submit: () -> Submit
    ) {
        self.init(
            isOpaque: isOpaque,
            leading: { EmptyView() },
            field: field,
            strip: strip,
            tools: tools,
            submit: submit
        )
    }
}

extension GlassComposer where Strip == EmptyView, Tools == EmptyView {
    /// A leading row and no tools — the support reply box.
    public init(
        isOpaque: Bool = false,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder field: () -> Field,
        @ViewBuilder submit: () -> Submit
    ) {
        self.init(
            isOpaque: isOpaque,
            leading: leading,
            field: field,
            strip: { EmptyView() },
            tools: { EmptyView() },
            submit: submit
        )
    }
}
