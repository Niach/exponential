import ExpCore
import ExpUI
import SwiftUI

// The "New trigger" / "Edit trigger" form (EXP-583). EXP-1291: a trigger
// belongs to the team AGENT whose Schedule rows open this, and the agent's own
// device fires it — so the sheet is the when-part alone (`TriggerForm`:
// schedule or event). Owner-only: the agent page hides the entry points for
// everyone else, and the server refuses anyway.
struct TriggerFormSheet: View {
    let teamId: String
    /// nil = add a new trigger.
    let editing: AgentTrigger?
    /// The configured trigger: `editing`'s id and enabled flag kept, a new
    /// one without an id (the server mints it) and enabled.
    let onSubmit: (AgentTriggerInput) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    @State private var draft = TriggerDraft()
    @State private var filterOptions = TriggerFilterOptions()
    @State private var seeded = false

    var body: some View {
        GlassSheetChrome(
            title: editing == nil ? "New trigger" : "Edit trigger",
            height: .full,
            content: {
                Form {
                    TriggerForm(draft: $draft, options: filterOptions)
                }
                // EXP-603: the sheet's own background shows through the
                // grouped list; rows carry the glass fill.
                .scrollContentBackground(.hidden)
                .listSectionSpacing(8)
                // EXP-594: white control tint — system blue is retired.
                .tint(DesignTokens.Palette.primary)
            },
            primaryAction: {
                GlassSubmitButton(editing == nil ? "Add trigger" : "Save changes") {
                    submit()
                }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("trigger-form-sheet")
        .onAppear { seed() }
        .task {
            filterOptions = await TriggerFilterOptions.load(
                db: deps.db, accountId: accountId, teamId: teamId
            )
        }
    }

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let editing { draft = TriggerDraft(trigger: editing.when) }
    }

    private func submit() {
        // Snapshot before dismissing — the payload must not depend on what
        // the teardown does to the sheet's state.
        let input = AgentTriggerInput(
            id: editing?.id,
            enabled: editing?.enabled ?? true,
            when: draft.trigger
        )
        dismiss()
        onSubmit(input)
    }
}
