import ExpCore
import ExpUI
import SwiftUI

/// Move to board (EXP-57): pick a same-team target, then confirm — the issue
/// is renumbered in the target board, so the move deserves an explicit yes
/// before it fires. The pick is parked by the caller and promoted once this
/// picker finished dismissing (a sheet cannot present while its sibling is
/// still animating away). EXP-893: its own file — the Work screen's `…` menu
/// and the Properties sheet's stacked child both present it.
struct MoveBoardPickerSheet: View {
    let boards: [BoardEntity]
    let selectedId: String
    let onSelect: (BoardEntity) -> Void

    var body: some View {
        GlassPickerSheet(
            title: "Move to board",
            items: boards,
            selectedID: selectedId,
            idFor: { $0.id },
            onSelect: onSelect
        ) { board in
            Label {
                Text(board.name)
            } icon: {
                // Board glyph tinted with the board color — same idiom as
                // the board switcher sheet (EXP-449).
                AppIcon(BoardTypeDisplay.iconName(for: board), size: 16)
                    .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
            }
        }
    }
}

/// The "Move issue" confirmation (EXP-57), reusable so BOTH paths — the `…`
/// menu's picker and the one Properties stacks over itself — confirm with the
/// exact same words on their own host node (EXP-687).
private struct MoveBoardConfirm: ViewModifier {
    @Binding var target: BoardEntity?
    let identifier: String?
    let onConfirm: (BoardEntity) -> Void

    func body(content: Content) -> some View {
        content.alert(
            "Move issue",
            isPresented: Binding(
                get: { target != nil },
                set: { if !$0 { target = nil } }
            ),
            presenting: target
        ) { board in
            Button("Move") { onConfirm(board) }
            Button("Cancel", role: .cancel) {}
        } message: { board in
            // Byte-shared with web, desktop and Android (EXP-426).
            Text("Move \(identifier ?? "this issue") to \"\(board.name)\"? The issue will get a new identifier in that board.")
        }
    }
}

extension View {
    func moveBoardConfirm(
        target: Binding<BoardEntity?>,
        identifier: String?,
        onConfirm: @escaping (BoardEntity) -> Void
    ) -> some View {
        modifier(MoveBoardConfirm(target: target, identifier: identifier, onConfirm: onConfirm))
    }
}
