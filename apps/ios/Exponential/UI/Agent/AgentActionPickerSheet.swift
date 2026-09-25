import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the composer's action picker — the ▶ tool's single-pick list:
/// builtins pinned FIRST by the `builtin` flag ("Fix merge conflicts", then
/// "Create action" — creation is a subject of this composer now, not a sheet
/// of its own), then the team's rows (`AgentComposerModel.actions` owns that
/// order). Chat is never listed: it is what "no subject" means. A tap picks
/// and dismisses; picking swaps out any issue chips.
///
/// EXP-1030: it renders through the SHARED `ActionPicker` — every row the
/// action's curated glyph (EXP-273) + its name + its description, in the one
/// sheet every other pick opens. HOST-DRIVEN (`open`): the composer's tool
/// button is the trigger, in another view tree.
struct AgentActionPickerSheet: View {
    let model: AgentComposerModel
    /// The composer's ▶ button drives it.
    @Binding var isPresented: Bool

    var body: some View {
        ActionPicker(
            // EXP-273: the action's own curated glyph (the builtins set one
            // too), falling back to the generic action mark — resolved by the
            // ONE row bridge, `ActionPickerAction(ActionDto)`.
            actions: model.actions.map(ActionPickerAction.init),
            value: model.selectedAction?.id,
            onChange: { id in
                guard let action = model.actions.first(where: { $0.id == id }) else { return }
                model.pickAction(action)
            },
            open: $isPresented,
            hideTrigger: true,
            sheetIdentifier: "agent-composer-actions-picker",
            trigger: { EmptyView() }
        )
    }
}
