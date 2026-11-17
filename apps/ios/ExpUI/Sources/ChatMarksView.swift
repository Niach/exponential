import ExpCore
import SwiftUI

/// #EDE-98 CHAT MARKS (chats.json `chatMarks`): the glyphs right after a
/// chat's title, in the chat header and on every Chats list row: the team
/// icon when the root is shared, the open lock when the tree has no key.
/// `shell.markGlyph` at the tertiary text colour, `shell.markGap` apart,
/// never tappable. No marks = nothing drawn.
public struct ChatMarksView: View {
    private let marks: [Chats.ChatMark]

    public init(marks: [Chats.ChatMark]) {
        self.marks = marks
    }

    public var body: some View {
        if !marks.isEmpty {
            HStack(spacing: CGFloat(Chats.Shell.markGap)) {
                ForEach(marks, id: \.self) { mark in
                    AppIcon(Self.icon(mark), size: CGFloat(Chats.Shell.markGlyph))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .accessibilityLabel(mark.label)
                        .accessibilityIdentifier(mark.testId)
                }
            }
            .fixedSize()
            .allowsHitTesting(false)
        }
    }

    /// The mark's concept glyph (`ChatMark.rawValue` = the concept id).
    public static func icon(_ mark: Chats.ChatMark) -> String {
        switch mark {
        case .team: AppIcons.uiTeam
        case .unencrypted: AppIcons.uiUnencrypted
        }
    }
}
