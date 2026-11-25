import ExpCore
import ExpUI
import SwiftUI

/// Move to board (EXP-57): pick a same-team target, then confirm — the issue
/// is renumbered in the target board, so the move deserves an explicit yes
/// before it fires. The pick is parked by the caller and promoted once this
/// picker finished dismissing (a sheet cannot present while its sibling is
/// still animating away). EXP-893: its own file — the Work screen's `…` menu
/// and the Properties sheet's stacked child both open it.
///
/// EXP-1021: it IS the shared `BoardPicker` now (board glyph in the board's
/// colour, plain rows, one sheet), host-DRIVEN because both entry points are
/// a menu item rather than a chip the picker could wrap.
///
/// #EXP-1832 (sidebar-folders.json): the rows fold into the member's board
/// folders, read-only (no drag, no editing), in the sidebar's order: boards
/// by sortOrder, the placed ones first.
struct MoveBoardPicker: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState: TeamState?
    @State private var layout = SidebarLayoutModel()

    let boards: [BoardEntity]
    let selectedId: String
    let open: Binding<Bool>
    var onDismiss: (() -> Void)?
    /// The sheet's title — the `…` item's words by default; the New issue
    /// page (EXP-1170) picks its draft's board, not a move.
    var title: String = "Move to board"
    let onSelect: (BoardEntity) -> Void

    /// One team's boards fold into its folders; a list over several teams is
    /// flat, in the caller's order, each row with its team's name (#EXP-2125).
    private var showsTeam: Bool { Set(boards.map(\.teamId)).count > 1 }
    private var teamId: String? { showsTeam ? nil : boards.first?.teamId }

    /// The sidebar's default order (`BoardOrder`).
    private var ordered: [BoardEntity] { showsTeam ? boards : BoardOrder.sorted(boards) }

    private func row(_ board: BoardEntity) -> BoardPickerBoard {
        BoardPickerBoard(board, teamName: showsTeam ? teamState?.team(board.teamId)?.name : nil)
    }

    var body: some View {
        BoardPicker(
            boards: ordered.map(row),
            value: selectedId,
            onChange: { picked in
                guard let board = boards.first(where: { $0.id == picked }) else { return }
                onSelect(board)
            },
            // The same words as the `…` item that opens it: the sheet must
            // not read as a plain board switch.
            title: title,
            open: open,
            hideTrigger: true,
            onDismiss: onDismiss,
            folders: teamId.map { BoardPickerFolders(teamId: $0, entries: layout.entries) },
            trigger: { EmptyView() }
        )
        .onAppear { bindLayout() }
        .onChange(of: teamId) { _, _ in bindLayout() }
    }

    private func bindLayout() {
        layout.bind(accountId: accountId, teamId: teamId, section: "board", db: deps.db)
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
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
        content.glassAlert(item: $target) { board in
            GlassAlert(
                prompt: Prompts.MoveIssue.copy(identifier: identifier ?? "this issue", board: board.name),
                handlers: ["move": { onConfirm(board) }]
            )
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
