import ExpCore
import SwiftUI
import UIKit

// EXP-1324: THE CodeBlock primitive (shadcn code-block, radix-nova) at size
// sm — tool output inside a Trace line's content. Geometry =
// `fixtures/trace.json` `geometry.codeBlock`: radius `lg`, a hairline in the
// input colour over the input colour at `fillAlpha`, the `Output` header
// with the ghost copy button (ui-copy → ui-check for 2s), mono code that
// scrolls sideways (never wraps) and at most `maxLines` lines tall. The
// destructive variant: a destructive hairline, destructive title + text, no
// fill. Language `ansi` colours command output (ExpCore `AnsiCode`); `plain`
// paints it as is.

public struct CodeBlockView: View {
    public enum Language: Sendable {
        case plain
        case ansi
    }

    let code: String
    let language: Language
    let title: String?
    let destructive: Bool
    /// Open scrolled to the LAST line (command output: the verdict is last).
    let scrollToEnd: Bool

    @State private var copied = false
    @State private var copyReset: Task<Void, Never>?

    private typealias G = Trace.Geometry.CodeBlock

    public init(
        _ code: String,
        language: Language = .plain,
        title: String? = Trace.Copy.output,
        destructive: Bool = false,
        scrollToEnd: Bool = true
    ) {
        self.code = code
        self.language = language
        self.title = title
        self.destructive = destructive
        self.scrollToEnd = scrollToEnd
    }

    private var lines: [[CodeToken]] {
        switch language {
        case .ansi:
            return AnsiCode.tokenize(code)
        case .plain:
            let text = code.replacingOccurrences(of: "\r\n", with: "\n")
            return text.split(separator: "\n", omittingEmptySubsequences: false).map {
                $0.isEmpty ? [] : [CodeToken(kind: "plain", text: String($0))]
            }
        }
    }

    /// What Copy copies: the shown text, escape-free.
    private var copyText: String {
        language == .ansi ? AnsiCode.text(code) : code.replacingOccurrences(of: "\r\n", with: "\n")
    }

    private func color(_ kind: String) -> Color {
        if destructive { return DesignTokens.Palette.destructive }
        switch kind {
        case "success": return DesignTokens.Diff.addFg
        case "error": return DesignTokens.Palette.destructive
        // Web `--hljs-number` (Android `codeTokenColor`), not the yellow.
        case "warning": return DesignTokens.Avatar.orange
        case "comment": return DesignTokens.Palette.mutedForeground
        default: return DesignTokens.Palette.foreground
        }
    }

    private var attributed: AttributedString {
        var out = AttributedString()
        for (index, line) in lines.enumerated() {
            if index > 0 { out += AttributedString("\n") }
            for token in line {
                var piece = AttributedString(token.text)
                piece.foregroundColor = color(token.kind)
                out += piece
            }
        }
        return out
    }

    private static let monoFont = UIFont.monospacedSystemFont(ofSize: G.fontSize, weight: .regular)
    private static let bottomId = "code-block-bottom"

    public var body: some View {
        let count = max(1, lines.count)
        let topPad = title == nil ? G.padding : G.contentTopWithHeader
        let visible = CGFloat(min(count, G.maxLines)) * G.lineHeight
        VStack(alignment: .leading, spacing: 0) {
            if let title { header(title) }
            ScrollViewReader { proxy in
                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 0) {
                        Text(attributed)
                            .font(Font(Self.monoFont))
                            .lineSpacing(max(0, G.lineHeight - Self.monoFont.lineHeight))
                            .padding(.vertical, (G.lineHeight - Self.monoFont.lineHeight) / 2)
                            .fixedSize()
                            .textSelection(.enabled)
                        Color.clear.frame(height: 0).id(Self.bottomId)
                    }
                    .padding(.horizontal, G.padding)
                    .padding(.top, topPad)
                    .padding(.bottom, G.padding)
                }
                .frame(height: visible + topPad + G.padding)
                .scrollBounceBehavior(.basedOnSize)
                .onAppear {
                    if scrollToEnd, count > G.maxLines {
                        proxy.scrollTo(Self.bottomId, anchor: .bottomLeading)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(destructive ? Color.clear : DesignTokens.Palette.input.opacity(G.fillAlpha))
        .clipShape(RoundedRectangle(cornerRadius: DesignTokens.Radius.lg, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: DesignTokens.Radius.lg, style: .continuous)
                .strokeBorder(
                    destructive
                        ? DesignTokens.Palette.destructive.opacity(G.destructiveBorderAlpha)
                        : DesignTokens.Palette.input,
                    lineWidth: G.border
                )
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("code-block")
        // The copied-check reset must not outlive the view.
        .onDisappear { copyReset?.cancel() }
    }

    private func header(_ title: String) -> some View {
        HStack(spacing: 0) {
            Text(verbatim: title)
                .font(.system(size: G.titleSize, weight: .medium))
                .foregroundStyle(destructive ? DesignTokens.Palette.destructive : DesignTokens.Palette.foreground)
                .lineLimit(1)
            Spacer(minLength: 8)
            Button(action: copy) {
                AppIcon(copied ? AppIcons.uiCheck : AppIcons.uiCopy, size: G.copyIcon)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
                    .frame(width: G.copyButton, height: G.copyButton)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(copied ? Trace.Copy.copied : Trace.Copy.copy)
            // Pulled into the right/top padding (`headerActionsOutset`).
            .padding(.vertical, -(G.copyButton - G.titleSize - 4) / 2)
            .offset(x: G.headerActionsOutset)
        }
        .frame(height: G.titleSize + 4)
        .padding(.horizontal, G.padding)
        .padding(.top, G.padding)
    }

    private func copy() {
        UIPasteboard.general.string = copyText
        copied = true
        copyReset?.cancel()
        copyReset = Task { @MainActor in
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled else { return }
            copied = false
        }
    }
}
