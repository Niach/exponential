import ExpUI
import ExpCore
import SwiftUI

/// Issue picker for "Mark as duplicate…" (masterplan §5e): the team's other
/// issues, searchable; selecting one atomically sets `duplicateOfId` +
/// `status = duplicate` via the caller. EXP-1021: it is the shared
/// `IssuePicker` (through `IssueCandidatePicker`, which owns the EXP-892
/// ranking), host-driven because the status picker HANDS OFF to it rather
/// than a trigger opening it.
struct DuplicateIssuePicker: View {
    /// Candidate canonical issues (same team, self excluded), newest first.
    let loadCandidates: () async -> [IssueEntity]
    let open: Binding<Bool>
    var onDismiss: (() -> Void)?
    /// EXP-892 — the picker's server augmentation: a debounced
    /// `issues.search` so a query that only matches a COMMENT still finds
    /// its issue. Hits outside `loadCandidates`' pool are dropped.
    var serverSearch: ((String) async -> [SearchIssueHit])?
    let onSelect: (IssueEntity) -> Void

    @State private var candidates: [IssueEntity]?

    var body: some View {
        IssueCandidatePicker(
            candidates: candidates,
            // The same words the banner the pick writes will use.
            title: "Duplicate of",
            open: open,
            onDismiss: onDismiss,
            serverSearch: serverSearch,
            onSelect: onSelect
        )
        // Loaded on demand: a picker the user never opens never touches the
        // store, and the sheet draws its loading row until the first pool
        // lands. Re-read on EVERY opening — the picker is permanently mounted
        // (the status picker hands off to it rather than a trigger opening
        // it), so a once-only load could never offer an issue created after
        // the first open. The previous pool stays on screen while the new one
        // is read: dropping it would flash the loading row on a re-open.
        .task(id: open.wrappedValue) {
            guard open.wrappedValue else { return }
            candidates = await loadCandidates()
        }
    }
}
