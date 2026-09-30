import ExpCore
import ExpUI
import SwiftUI

/// EXP-1150: the Work screen's ONE Merge PR — a compact primary pill at the
/// face tabs' height, trailing the tab row in the header band on EVERY face.
/// It took over the Changes bar's pill (and the Run bar's circle): the same
/// flow — the confirm alert, or the stack dialog for a stack member with other
/// open members (EXP-1145) — and, after a merge the server refused on a REAL
/// content conflict, the same "Fix conflicts" recovery run (EXP-706) in its
/// place. The host resolves WHAT it merges (`target`): the run's own merge
/// target while the run can merge, else the issue's open PR.
struct WorkMergePill: View {
    let target: MergeTarget
    /// The issue behind an `.issue` target — its PR number and branch.
    let issue: IssueEntity?
    /// The stack pool (`PrGraphModel.prIssues`) the stack dialog reads.
    let prIssues: [IssueEntity]
    /// Remote start is on — the recovery run can be launched.
    let steerEnabled: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    @State private var showMergeConfirm = false
    @State private var stackChoice: PrStack.StackMergeChoice?
    @State private var merging = false
    @State private var mergeFailure: MergeFailure?

    var body: some View {
        pill
        .alert("Merge pull request?", isPresented: $showMergeConfirm) {
            Button("Merge", role: .destructive) { merge() }
            Button("Cancel", role: .cancel) {}
        } message: {
            // EXP-734: a run's OWN pull request links no issue, so promising
            // completed issues would be a lie.
            if case .session = target {
                Text("Merges this run's pull request and closes the coding session.")
            } else if let number = issue?.prNumber {
                Text("Squash-merges PR #\(number), completes every linked issue, and closes any live coding session for it.")
            } else {
                Text("Merges the pull request, completes every linked issue, and closes the coding session.")
            }
        }
        .confirmationDialog(
            PrStack.stackMergeChoiceTitle,
            isPresented: Binding(
                get: { stackChoice != nil },
                set: { if !$0 { stackChoice = nil } }
            ),
            titleVisibility: .visible,
            presenting: stackChoice
        ) { choice in
            Button(PrStack.mergeStackLabel) { mergeStack(topIssueId: choice.topIssueId) }
            Button(PrStack.mergeThisPrLabel) { merge() }
            Button(PrStack.stackMergeCancelLabel, role: .cancel) {}
        } message: { choice in
            Text(choice.body)
        }
    }

    private var pill: some View {
        let fix = canFixConflicts
        let action: () -> Void = fix ? { openFixConflicts() } : { requestMerge() }
        return Button(action: action) {
            pillLabel(fix: fix)
        }
        .buttonStyle(.plain)
        .disabled(merging)
        .fixedSize()
        .accessibilityLabel(fix ? "Fix merge conflicts" : DomainContract.diffUiMergePr)
        .accessibilityIdentifier("work-merge-pr")
    }

    private func pillLabel(fix: Bool) -> some View {
        HStack(spacing: 6) {
            if merging {
                ProgressView()
                    .controlSize(.small)
                    .tint(DesignTokens.Palette.primaryForeground)
            } else {
                AppIcon(
                    fix ? AppIcons.uiBranch : AppIcons.prMerged,
                    size: AppIcon.Size.small,
                    weight: .medium
                )
            }
            Text(fix ? "Fix conflicts" : DomainContract.diffUiMergePr)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .foregroundStyle(DesignTokens.Palette.primaryForeground)
        .padding(.horizontal, 14)
        .frame(height: GlassSegmentedControlTokens.height)
        .background(DesignTokens.Palette.primary, in: Capsule())
        .contentShape(Capsule())
    }

    /// Only a REAL content conflict on an ISSUE-linked PR with a recorded
    /// branch gets the recovery run (EXP-533/734).
    private var canFixConflicts: Bool {
        guard case .issue = target else { return false }
        return steerEnabled
            && mergeFailure?.isConflict == true
            && !(issue?.branch ?? "").isEmpty
    }

    private func requestMerge() {
        if case let .issue(issueId) = target,
           let row = issue?.id == issueId ? issue : prIssues.first(where: { $0.id == issueId }),
           let choice = PrStack.stackMergeChoice(row, issues: prIssues) {
            stackChoice = choice
        } else {
            showMergeConfirm = true
        }
    }

    /// No local surgery on success: the server ends the run and flips
    /// `pr_state`, and the pill leaves when that syncs back.
    private func merge() {
        mergeFailure = nil
        merging = true
        let target = target
        Task {
            do {
                switch target {
                case let .issue(issueId):
                    try await deps.issuesApi.mergePr(accountId: accountId, issueId: issueId)
                case let .session(sessionId):
                    try await deps.codingSessionsApi.mergePr(accountId: accountId, sessionId: sessionId)
                }
            } catch {
                let failure = MergeFailure(error: error)
                mergeFailure = failure
                toaster.error(failure.message)
            }
            merging = false
        }
    }

    /// EXP-1145: "Merge stack" merges the whole stack through its TOP member.
    private func mergeStack(topIssueId: String) {
        mergeFailure = nil
        merging = true
        Task {
            do {
                try await deps.issuesApi.mergePr(
                    accountId: accountId, issueId: topIssueId, mergeStack: true
                )
            } catch {
                let failure = MergeFailure(error: error)
                mergeFailure = failure
                toaster.error(failure.message)
            }
            merging = false
        }
    }

    /// EXP-825: the recovery run is the Agent page composer with the "Fix
    /// merge conflicts" builtin picked and THIS pull request pre-picked.
    private func openFixConflicts() {
        guard case let .issue(issueId) = target else { return }
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: issueId
            )
        ))
    }
}
