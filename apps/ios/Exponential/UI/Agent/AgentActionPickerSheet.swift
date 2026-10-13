import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the composer's action picker, the "+" menu's Run action › page:
/// the team's saved prompts (`AgentComposerModel.actions` owns the order;
/// EXP-1291: no builtins). A pick sets the subject and closes the menu;
/// picking swaps out any issue chips.
///
/// SLOP-28 (EXP-1259 D5): THE search list (`SearchListView`), pushed as a
/// page of the "+" sheet; a row is the action's curated glyph (EXP-273) + its
/// NAME only, no description (composer-menu.json).
struct AgentActionPickerPage: View {
    let model: AgentComposerModel
    /// Closes the whole "+" menu.
    let onClose: () -> Void

    @State private var searchText = ""

    var body: some View {
        SearchListView(
            items: model.actions.map { action in
                PickerItem(
                    value: action.id,
                    label: action.name,
                    icon: ActionIconDisplay.iconName(for: action.icon)
                )
            },
            query: $searchText,
            placeholder: "Search actions",
            mode: .single,
            value: Set(model.selectedAction.map { [$0.id] } ?? []),
            filters: true,
            inputIdentifier: ComposerMenu.submenuSearchTestId,
            emptyText: "No matching actions",
            onPick: { id in
                guard let action = model.actions.first(where: { $0.id == id }) else { return }
                model.pickAction(action)
                onClose()
            }
        )
        .accessibilityIdentifier("agent-composer-actions-picker")
    }
}
