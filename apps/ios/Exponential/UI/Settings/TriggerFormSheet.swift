import ExpCore
import ExpUI
import SwiftUI

// The "New trigger" / "Edit trigger" form (EXP-583). NEW-2: a trigger
// belongs to the team AGENT definition whose Triggers rows open this, and the
// SERVER fires it — so the sheet is the when-part alone (`TriggerForm`: a
// cron schedule or an event name). Owner-only: the agent page hides the entry
// points for everyone else, and the server refuses anyway.
struct TriggerFormSheet: View {
    let teamId: String
    /// nil = add a new trigger.
    let editing: AgentTrigger?
    /// The configured trigger: `editing`'s id and enabled flag kept, a new
    /// one without an id (the server mints it) and enabled.
    let onSubmit: (AgentTriggerInput) -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var draft = TriggerDraft()
    @State private var seeded = false

    var body: some View {
        GlassSheetChrome(
            title: editing == nil ? "New trigger" : "Edit trigger",
            height: .full,
            content: {
                Form {
                    TriggerForm(draft: $draft)
                }
                // EXP-603: the sheet's own background shows through the
                // grouped list; rows carry the glass fill.
                .scrollContentBackground(.hidden)
                .listSectionSpacing(8)
                // EXP-594: white control tint — system blue is retired.
                .tint(DesignTokens.Palette.primary)
            },
            primaryAction: {
                GlassSubmitButton(editing == nil ? "Add trigger" : "Save changes", enabled: draft.isValid) {
                    submit()
                }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("trigger-form-sheet")
        .onAppear { seed() }
    }

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let editing { draft = TriggerDraft(trigger: editing.when) }
    }

    private func submit() {
        guard draft.isValid else { return }
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
