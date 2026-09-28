import ExpUI
import SwiftUI
import UIKit
@preconcurrency import VappSpikeKit

/// `#hex` / `$palette.*` / `$semantic.*` → a design-token Color.
enum VappColor {
    static func resolve(_ raw: String?) -> Color? {
        guard let raw else { return nil }
        if raw.hasPrefix("#") { return Color(hex: raw) }
        if raw.hasPrefix("$palette.") { return palette[String(raw.dropFirst("$palette.".count))] }
        if raw.hasPrefix("$semantic.") { return semantic[String(raw.dropFirst("$semantic.".count))] }
        return nil
    }

    private static let palette: [String: Color] = [
        "background": DesignTokens.Palette.background,
        "foreground": DesignTokens.Palette.foreground,
        "card": DesignTokens.Palette.card,
        "cardForeground": DesignTokens.Palette.cardForeground,
        "popover": DesignTokens.Palette.popover,
        "primary": DesignTokens.Palette.primary,
        "primaryForeground": DesignTokens.Palette.primaryForeground,
        "secondary": DesignTokens.Palette.secondary,
        "secondaryForeground": DesignTokens.Palette.secondaryForeground,
        "muted": DesignTokens.Palette.muted,
        "mutedForeground": DesignTokens.Palette.mutedForeground,
        "accent": DesignTokens.Palette.accent,
        "accentForeground": DesignTokens.Palette.accentForeground,
        "destructive": DesignTokens.Palette.destructive,
        "border": DesignTokens.Palette.border,
        "input": DesignTokens.Palette.input,
        "ring": DesignTokens.Palette.ring,
    ]

    private static let semantic: [String: Color] = [
        "neutral": DesignTokens.Semantic.neutral,
        "yellow": DesignTokens.Semantic.yellow,
        "green": DesignTokens.Semantic.green,
        "red": DesignTokens.Semantic.red,
        "orange": DesignTokens.Semantic.orange,
        "blue": DesignTokens.Semantic.blue,
    ]

    static func weight(_ value: UInt16) -> Font.Weight {
        switch value {
        case ..<200: .ultraLight
        case ..<300: .thin
        case ..<400: .light
        case ..<500: .regular
        case ..<600: .medium
        case ..<700: .semibold
        case ..<800: .bold
        case ..<900: .heavy
        default: .black
        }
    }

    static func uiWeight(_ value: UInt16) -> UIFont.Weight {
        switch value {
        case ..<400: .light
        case ..<500: .regular
        case ..<600: .medium
        case ..<700: .semibold
        default: .bold
        }
    }
}

/// One node of the surface, painted from its `PlacedFrame` (visual + font).
struct VappNodeView: View {
    let node: VappNode
    let model: VappSurfaceModel

    var body: some View {
        let paint = model.paint[node.index]
        content(paint)
            .opacity(Double(paint?.visual.opacity ?? 1))
            .modifier(VappAncestorClip(node: node, model: model))
            // SwiftUI orders a Layout's elements GEOMETRICALLY (row by row),
            // not by subview order; force the fixture's pre-order. Applied
            // OUTERMOST so it lands on the Layout's direct child.
            .accessibilitySortPriority(Double(model.nodes.count - node.index))
            .accessibilityHidden(node.isContainer || node.kind == "avatar" || node.kind == "divider")
    }

    @ViewBuilder
    private func content(_ paint: PlacedFrame?) -> some View {
        if node.isContainer {
            container(paint)
                .accessibilityHidden(true)
        } else {
            leaf(paint)
                .modifier(VappLeafClip(clips: paint?.visual.overflowHidden ?? false))
                .modifier(VappLeafAccessibility(node: node))
        }
    }

    // MARK: Containers

    @ViewBuilder
    private func container(_ paint: PlacedFrame?) -> some View {
        let visual = paint?.visual
        let radius = CGFloat(visual?.borderRadius ?? 0)
        if node.kind == "card" {
            // `card` = the platform card: `.glassCard()` (fillCard + strokeCard
            // hairline, radius xl = 16) on a clear shape.
            Color.clear.glassCard()
        } else {
            RoundedRectangle(cornerRadius: radius)
                .fill(VappColor.resolve(visual?.backgroundColor) ?? .clear)
                .overlay {
                    if let width = visual?.borderWidth, width > 0 {
                        RoundedRectangle(cornerRadius: radius)
                            .stroke(VappColor.resolve(visual?.borderColor) ?? GlassTokens.strokeCard, lineWidth: CGFloat(width))
                    }
                }
        }
    }

    // MARK: Leaves

    @ViewBuilder
    private func leaf(_ paint: PlacedFrame?) -> some View {
        let props = node.props
        switch node.kind {
        case "text":
            VappText(text: props.text ?? "", variant: props.variant ?? "body", paint: paint)
        case "button":
            VappButton(id: node.id, label: props.label ?? "", variant: props.variant ?? "primary", model: model)
        case "textfield":
            if props.echo {
                VappEchoField(placeholder: props.placeholder ?? "")
            } else {
                VappPlainField(placeholder: props.placeholder ?? "", lines: nil)
            }
        case "textarea":
            VappPlainField(placeholder: props.placeholder ?? "", lines: 3...6)
        case "toggle":
            VappToggle(label: props.label ?? "", initial: props.checked)
        case "select":
            VappSelect(options: props.options, initial: props.value ?? props.options.first ?? "")
        case "listrow":
            HStack(spacing: 8) {
                Text(props.title ?? "")
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 8)
                Text(props.meta ?? "")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            .padding(.horizontal, 8)
            .frame(minHeight: DesignTokens.Size.rowHeight)
            .flatRow()
            .accessibilityElement(children: .ignore)
        case "badge":
            Text("\(props.count ?? 0)")
                .font(.caption2.weight(.semibold).monospacedDigit())
                .foregroundStyle(DesignTokens.Palette.primaryForeground)
                .padding(.horizontal, 6)
                .frame(minWidth: 20, minHeight: 20)
                .background(Capsule().fill(DesignTokens.Palette.primary))
                .fixedSize()
        case "pill":
            GlassPill(
                props.label ?? "",
                mode: props.tone == nil ? .select(isSelected: props.selected, action: {}) : .readonly,
                dot: props.tone == "live" ? DesignTokens.Semantic.green : nil,
                isOpaque: props.tone == "live"
            )
            .fixedSize()
        case "avatar":
            let name = props.name ?? "?"
            UserAvatar(
                image: nil,
                initials: name.split(separator: " ").compactMap(\.first).prefix(2).map(String.init).joined(),
                hueKey: name,
                size: CGFloat(props.size ?? 32)
            )
        case "image":
            (VappColor.resolve(props.placeholder) ?? DesignTokens.Semantic.blue)
                .opacity(0.55)
                .overlay {
                    AppIcon("image", size: 28)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
        case "divider":
            GlassDivider()
        case "progress":
            ProgressView(value: props.progress ?? 0)
                .progressViewStyle(.linear)
                .tint(DesignTokens.Palette.primary)
        case "markdown":
            AgentMarkdownText(text: props.text ?? "")
        default:
            Color.clear
        }
    }
}

/// LANES.md step 5: every leaf carries a label (text/label/title/placeholder).
/// Fields keep their own (placeholder + value); decorations are hidden.
private struct VappLeafAccessibility: ViewModifier {
    let node: VappNode

    func body(content: Content) -> some View {
        switch node.kind {
        case "textfield", "textarea":
            content
        case "avatar", "divider":
            content.accessibilityHidden(true)
        case "image":
            content.accessibilityLabel(node.props.alt ?? "Image").accessibilityAddTraits(.isImage)
        case "progress":
            content.accessibilityLabel("Progress")
        case "markdown":
            content
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(Self.plain(node.props.text ?? ""))
        default:
            content.accessibilityLabel(node.props.accessibilityText ?? node.id)
        }
    }

    static func plain(_ markdown: String) -> String {
        let attributed = try? AttributedString(
            markdown: markdown,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )
        return attributed.map { String($0.characters) } ?? markdown
    }
}

/// `overflow: hidden` on a leaf clips the leaf itself.
private struct VappLeafClip: ViewModifier {
    let clips: Bool
    func body(content: Content) -> some View {
        if clips { content.clipped() } else { content }
    }
}

/// The painters are FLAT (every node is a sibling subview), so an ancestor's
/// `overflow: hidden` + radius cannot clip descendants by nesting. Each
/// descendant masks itself with the ancestor's rounded rect instead, read in
/// the surface coordinate space after placement.
private struct VappAncestorClip: ViewModifier {
    let node: VappNode
    let model: VappSurfaceModel

    func body(content: Content) -> some View {
        if let ancestor = clippingAncestor {
            content.mask(alignment: .topLeading) {
                GeometryReader { proxy in
                    let me = proxy.frame(in: .named(VappKitchenSinkView.coordinateSpace))
                    let rect = model.placedRect(ancestor) ?? me
                    let radius = CGFloat(model.paint[ancestor]?.visual.borderRadius ?? 0)
                    RoundedRectangle(cornerRadius: radius)
                        .frame(width: rect.width, height: rect.height)
                        .offset(x: rect.minX - me.minX, y: rect.minY - me.minY)
                }
                .environment(\.layoutDirection, .leftToRight)
            }
        } else {
            content
        }
    }

    private var clippingAncestor: Int? {
        var cursor = node.parent
        while let index = cursor {
            if model.paint[index]?.visual.overflowHidden == true { return index }
            cursor = model.nodes[index].parent
        }
        return nil
    }
}

// MARK: - Leaf controls

private struct VappText: View {
    let text: String
    let variant: String
    let paint: PlacedFrame?

    var body: some View {
        let size = CGFloat(paint?.fontSize ?? 14)
        let weight = paint?.fontWeight ?? 400
        let lineHeight = CGFloat(paint?.lineHeight ?? 20)
        let natural = UIFont.systemFont(ofSize: size, weight: VappColor.uiWeight(weight)).lineHeight
        Text(text)
            .font(.system(size: size, weight: VappColor.weight(weight)))
            .lineSpacing(max(0, lineHeight - natural))
            // Half-leading above and below: n lines = n × lineHeight, like CSS.
            .padding(.vertical, max(0, lineHeight - natural) / 2)
            .foregroundStyle(color)
            .multilineTextAlignment(alignment)
    }

    private var color: Color {
        if let explicit = VappColor.resolve(paint?.visual.color) { return explicit }
        switch variant {
        case "muted": return DesignTokens.Palette.mutedForeground
        case "label": return .white.opacity(TextOpacity.secondary)
        default: return DesignTokens.Palette.foreground
        }
    }

    private var alignment: TextAlignment {
        switch paint?.visual.textAlign {
        case "center": .center
        case "right", "end": .trailing
        default: .leading
        }
    }
}

/// Reports press-down/up to the model → `set_pressed` → relayout. The pressed
/// VISUAL (opacity 0.6) comes back through the core's `PlacedFrame.visual`.
private struct VappPressReportingStyle: ButtonStyle {
    let id: String
    let model: VappSurfaceModel
    let variant: String

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .onChange(of: configuration.isPressed) { _, pressed in
                if pressed { model.pressed.insert(id) } else { model.pressed.remove(id) }
            }
    }
}

private struct VappButton: View {
    let id: String
    let label: String
    let variant: String
    let model: VappSurfaceModel

    var body: some View {
        Button {} label: {
            Text(label)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
                .foregroundStyle(variant == "primary" ? DesignTokens.Palette.primaryForeground : .white)
                .padding(.horizontal, variant == "ghost" ? 10 : 14)
                .frame(height: DesignTokens.Size.controlLg)
                .background {
                    switch variant {
                    case "primary":
                        Capsule().fill(DesignTokens.Palette.primary)
                    case "outline":
                        Capsule().fill(GlassTokens.fillCard)
                            .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
                    default:
                        Color.clear
                    }
                }
                .contentShape(Capsule())
        }
        .buttonStyle(VappPressReportingStyle(id: id, model: model, variant: variant))
        .fixedSize()
        .accessibilityIdentifier("vapp-\(id)")
    }
}

private struct VappPlainField: View {
    let placeholder: String
    let lines: ClosedRange<Int>?
    @State private var text = ""

    var body: some View {
        GlassTextField(placeholder, text: $text, lines: lines, verticalPadding: 8)
            .font(.subheadline)
            .accessibilityLabel(placeholder)
    }
}

/// The fake host behind the typing test: receives every client edit with the
/// client's revision and echoes it back 150 ms later; a stale echo (an older
/// revision) is dropped. A reference type OWNED OUTSIDE the field, so an echo
/// re-renders only the label that observes it.
@MainActor
@Observable
final class VappEchoHost {
    private(set) var echoed = ""
    private(set) var revision = 0
    private(set) var dropped = 0
    private(set) var clientCount = 0
    /// A real host CHANGE for the field (never an echo of the same value);
    /// the field applies it only on its latest revision.
    private(set) var write: VappHostWrite?

    /// A client edit. The field owns the text; the host only records it.
    func clientEdited(_ value: String, revision sent: Int) {
        revision = sent
        clientCount = value.count
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard let self else { return }
            guard sent == self.revision else {
                self.dropped += 1
                return
            }
            // The echo carries the value the client already holds: display
            // only, no write into the field.
            self.echoed = value
        }
    }

    /// The host rewrites the value (not used by the fixture; the path the
    /// real product needs, gated in `VappOwnedTextField.updateUIView`).
    func hostChanged(_ value: String) {
        write = VappHostWrite(value: value, revision: revision)
    }
}

/// The typing test (LANES.md): the CLIENT owns the text (a UIKit-owned field,
/// FINDINGS-ios.md fix attempt); every edit goes to the host with a revision,
/// and the host's echo shows in a separate label.
struct VappEchoField: View {
    let placeholder: String
    @State private var host = VappEchoHost()
    @State private var focused = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            VappOwnedTextField(
                placeholder: placeholder,
                accessibilityIdentifier: "echo-field",
                hostWrite: host.write,
                onEdit: { value, revision in host.clientEdited(value, revision: revision) },
                focused: $focused
            )
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: GlassTokens.fieldRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                    .stroke(focused ? GlassTokens.strokeActive : GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
            VappEchoHostLabel(host: host)
        }
    }
}

/// The only view that observes the host: an echo re-renders just this.
private struct VappEchoHostLabel: View {
    let host: VappEchoHost

    var body: some View {
        Text("host: \(host.echoed)")
            .font(.caption)
            .foregroundStyle(DesignTokens.Palette.mutedForeground)
            .lineLimit(1)
            .accessibilityIdentifier("echo-host")
            .accessibilityLabel("host: \(host.echoed)")
            // Typing-test diagnostics: edits seen, stale echoes dropped,
            // and the client text's length as the host last saw it.
            .accessibilityValue("revision \(host.revision) dropped \(host.dropped) state \(host.clientCount)")
    }
}

private struct VappToggle: View {
    let label: String
    @State private var isOn: Bool

    init(label: String, initial: Bool) {
        self.label = label
        _isOn = State(initialValue: initial)
    }

    var body: some View {
        Toggle(isOn: $isOn) {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white)
        }
        .toggleStyle(GlassToggleStyle())
    }
}

private struct VappSelect: View {
    let options: [String]
    @State private var value: String

    init(options: [String], initial: String) {
        self.options = options
        _value = State(initialValue: initial)
    }

    var body: some View {
        Menu {
            Picker("", selection: $value) {
                ForEach(options, id: \.self) { Text($0).tag($0) }
            }
        } label: {
            HStack(spacing: 8) {
                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 0)
                AppIcon("chevrons-up-down", size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .frame(height: DesignTokens.Size.inputHeight)
            .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: GlassTokens.fieldRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
        }
        .accessibilityLabel(value)
    }
}
