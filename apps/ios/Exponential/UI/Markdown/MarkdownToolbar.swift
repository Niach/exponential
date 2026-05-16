import SwiftUI

struct MarkdownToolbar: View {
    @Binding var text: String

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                toolButton("bold", symbol: "bold") { wrapSelection(prefix: "**", suffix: "**") }
                toolButton("italic", symbol: "italic") { wrapSelection(prefix: "_", suffix: "_") }
                toolButton("strikethrough", symbol: "strikethrough") { wrapSelection(prefix: "~~", suffix: "~~") }
                toolButton("code", symbol: "chevron.left.forwardslash.chevron.right") { wrapSelection(prefix: "`", suffix: "`") }

                Divider().frame(height: 20).background(Color.white.opacity(0.1))

                toolButton("h1", symbol: "textformat.size.larger") { insertPrefix("# ") }
                toolButton("h2", symbol: "textformat.size") { insertPrefix("## ") }
                toolButton("h3", symbol: "textformat.size.smaller") { insertPrefix("### ") }

                Divider().frame(height: 20).background(Color.white.opacity(0.1))

                toolButton("bullet", symbol: "list.bullet") { insertPrefix("- ") }
                toolButton("numbered", symbol: "list.number") { insertPrefix("1. ") }
                toolButton("quote", symbol: "text.quote") { insertPrefix("> ") }

                Divider().frame(height: 20).background(Color.white.opacity(0.1))

                toolButton("link", symbol: "link") { wrapSelection(prefix: "[", suffix: "](url)") }
                toolButton("image", symbol: "photo") { text += "\n![alt](url)" }
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 6)
        }
    }

    @ViewBuilder
    private func toolButton(_ label: String, symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 28)
        }
        .buttonStyle(.plain)
        .glassButton()
    }

    private func wrapSelection(prefix: String, suffix: String) {
        text += "\(prefix)text\(suffix)"
    }

    private func insertPrefix(_ prefix: String) {
        if text.isEmpty || text.hasSuffix("\n") {
            text += prefix
        } else {
            text += "\n\(prefix)"
        }
    }
}
