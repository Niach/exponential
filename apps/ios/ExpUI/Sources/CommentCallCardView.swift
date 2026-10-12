import ExpCore
import SwiftUI

// EXP-1241 (EXP-1259 D14): an `exponential_comments_create` call in a run's
// transcript renders the REAL comment, in the issue timeline's comment look:
// "Commented on <issue chip>" over a card holding the author row (the agent
// mark, the name, "via MCP · time") and the markdown body clamped to three
// lines behind "Show more". Tapping the card opens the issue scrolled to that
// comment. The host renders the chip, the mark and the body; this only lays
// them out. Web `@exp/ui` `CommentCallCard`, desktop `ui::comments`, Android
// `CommentCallCard.kt`.
public enum CommentCallCopy {
    public static let done = "Commented on"
    public static let running = "Commenting on"
    public static let showMore = "Show more"
    public static let showLess = "Show less"
}

public struct CommentCallCardView<Issue: View, Mark: View, Content: View>: View {
    let author: String?
    /// The muted caption after the name: "via MCP · 21:02".
    let caption: String?
    let running: Bool
    /// Open the issue scrolled to the comment; nil = inert.
    let onOpen: (() -> Void)?
    let issue: Issue
    let mark: Mark
    /// The rendered markdown body; the host passes `EmptyView` while the call
    /// runs (`hasBody` false).
    let content: Content
    let hasBody: Bool

    @State private var expanded = false
    @State private var fullHeight: CGFloat = 0
    /// Three body lines (web: 4.5em at the chat markdown's 1.5 line height).
    @ScaledMetric(relativeTo: .subheadline) private var clampHeight: CGFloat = 63

    public init(
        author: String?,
        caption: String?,
        running: Bool,
        hasBody: Bool,
        onOpen: (() -> Void)?,
        @ViewBuilder issue: () -> Issue,
        @ViewBuilder mark: () -> Mark,
        @ViewBuilder body: () -> Content
    ) {
        self.author = author
        self.caption = caption
        self.running = running
        self.hasBody = hasBody
        self.onOpen = onOpen
        self.issue = issue()
        self.mark = mark()
        self.content = body()
    }

    private var overflows: Bool { fullHeight > clampHeight + 1 }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                FeedbackMark(size: 14, color: muted, animated: false)
                Text(running ? CommentCallCopy.running : CommentCallCopy.done)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .fixedSize()
                if running { InlineSpinner(size: 12) }
                issue
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            if hasBody {
                if let onOpen {
                    Button(action: onOpen) { commentBox }
                        .buttonStyle(PressableRowStyle())
                        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
                } else {
                    commentBox
                }
            }
        }
        .padding(12)
        .background(GlassTokens.fillRow)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("comment-call-card")
    }

    private var commentBox: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                mark
                if let author {
                    Text(author)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                }
                if let caption {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(muted)
                        .lineLimit(1)
                        .fixedSize()
                }
                Spacer(minLength: 0)
            }
            content
                .fixedSize(horizontal: false, vertical: true)
                .background(GeometryReader { proxy in
                    Color.clear
                        .onAppear { fullHeight = proxy.size.height }
                        .onChange(of: proxy.size.height) { _, height in fullHeight = height }
                })
                .frame(maxHeight: expanded ? nil : clampHeight, alignment: .top)
                .clipped()
            if overflows || expanded {
                Button(expanded ? CommentCallCopy.showLess : CommentCallCopy.showMore) {
                    expanded.toggle()
                }
                .font(.caption)
                .foregroundStyle(muted)
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GlassTokens.fillRow)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                .stroke(GlassTokens.strokeRow, lineWidth: GlassTokens.hairline)
        )
        .contentShape(Rectangle())
    }

    private var muted: Color { .white.opacity(TextOpacity.tertiary) }
}
