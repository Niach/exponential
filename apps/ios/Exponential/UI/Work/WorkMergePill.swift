import ExpCore
import ExpUI
import SwiftUI

/// EXP-1150: the Work screen's ONE Merge PR. EXP-1154: back on the FLOATING
/// BOTTOM BAR as the SOLID WHITE capsule (`FloatingBarSolidPill`, hugging its
/// label, 52pt: the EXP-916 Reviews look), on every face — the centre of the
/// Changes bar's cluster `[files] [Merge PR]` and alone on the Results bar.
/// EXP-1191: the Issue and Run bars wear it as a 52pt glass CIRCLE (`style:
/// .circle`, the merge glyph alone) right of their centre capsule, the
/// floating capsule above them retired. The same flow
/// everywhere: the confirm alert, or the stack dialog for a member of an open
/// PR stack (EXP-1145 `PrStack.stackMergeChoice`) and, after a merge the
/// server refused on a REAL content conflict, the "Fix conflicts" recovery
/// run (EXP-706) in its place. The host resolves WHAT it merges (`target`):
/// the run's own merge target while the run can merge, else the issue's open
/// PR, and mounts the capsule only then.
struct WorkMergePill: View {
    let target: MergeTarget
    /// The issue behind an `.issue` target — its PR number and branch.
    let issue: IssueEntity?
    /// The stack pool the stack dialog reads: the pull requests of the
    /// issue's OWN team (`PrGraphModel.stackPool`).
    let prIssues: [IssueEntity]
    /// Remote start is on — the recovery run can be launched.
    let steerEnabled: Bool
    /// A `.session` target's own PR number, for the confirm's title.
    var runPrNumber: Int?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    /// EXP-1154: the merge in flight and the last refusal are the SCREEN's
    /// (`WorkMergeState`): each face mounts its own capsule, and a conflict
    /// met on one face must offer Fix conflicts on all of them.
    @Binding var state: WorkMergeState
    /// The capsule's UI-test handle: `work-merge-pr` on the Changes face (the
    /// store slide's pop-out), suffixed with the face elsewhere, since the
    /// pager keeps neighbouring faces mounted.
    var identifier = "work-merge-pr"
    /// EXP-1191: the white capsule (Changes / Results clusters) or the
    /// icon-only bar circle (Issue / Run bars).
    var style: Style = .capsule
    enum Style { case capsule, circle }
    @State private var showMergeConfirm = false
    @State private var stackChoice: PrStack.StackMergeChoice?

    private var merging: Bool { state.merging }
    private var mergeFailure: MergeFailure? { state.failure }

    var body: some View {
        Group {
            switch style {
            case .capsule: pill
            case .circle: circle
            }
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
        .glassAlert(isPresented: $showMergeConfirm) {
            GlassAlert(prompt: mergePrompt, handlers: ["merge": { merge() }])
        }
        // EXP-1145: the stack dialog, its copy byte-locked by the
        // `stack-merge-choice.json` fixture.
        .glassAlert(item: $stackChoice) { choice in
            GlassAlert(
                title: PrStack.stackMergeChoiceTitle,
                message: choice.body,
                actions: [
                    GlassAlertAction(PrStack.stackMergeCancelLabel, role: .outline, id: "cancel") {},
                    GlassAlertAction(PrStack.mergeThisPrLabel, role: .outline, id: "merge-this") {
                        mergeThis(choice)
                    },
                    GlassAlertAction(PrStack.mergeStackLabel, role: .primary, id: "merge-stack") {
                        mergeStack(issueId: choice.topIssueId)
                    },
                ]
            )
        }
    }

    /// EXP-734: a run's OWN pull request links no issue (`merge-run-pr`);
    /// an issue's names how many issues it covers (a batch shares one PR).
    private var mergePrompt: PromptCopy {
        if case .session = target {
            return Prompts.MergeRunPr.copy(number: runPrNumber)
        }
        let url = issue?.prUrl
        let linked = url.map { url in prIssues.filter { $0.prUrl == url }.count } ?? 0
        return Prompts.MergeIssuePr.copy(number: issue?.prNumber, issueCount: max(1, linked))
    }

    private var pill: some View {
        let fix = canFixConflicts
        return FloatingBarSolidPill(
            accessibilityLabel: fix ? "Fix merge conflicts" : DomainContract.diffUiMergePr,
            enabled: !merging,
            action: fix ? { openFixConflicts() } : { requestMerge() }
        ) {
            if merging {
                ProgressView().controlSize(.small).tint(.black.opacity(0.6))
            } else {
                AppIcon(
                    fix ? AppIcons.uiBranch : AppIcons.prMerged,
                    size: FloatingBarTokens.glyph,
                    weight: .medium
                )
            }
            Text(fix ? "Fix conflicts" : DomainContract.diffUiMergePr)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .fixedSize()
        .accessibilityIdentifier(identifier)
    }

    /// EXP-1191: the bar circle — the SAME chrome as the bar's other circles,
    /// the merge glyph alone (the branch glyph once a conflict swaps in Fix
    /// conflicts), a spinner while the merge is in flight.
    private var circle: some View {
        let fix = canFixConflicts
        return FloatingBarCircle(
            accessibilityLabel: fix ? "Fix merge conflicts" : "Merge PR",
            enabled: !merging,
            action: fix ? { openFixConflicts() } : { requestMerge() }
        ) {
            if merging {
                ProgressView().controlSize(.small).tint(.white)
            } else {
                AppIcon(
                    fix ? AppIcons.uiBranch : AppIcons.prMerged,
                    size: FloatingBarTokens.glyph,
                    weight: .medium
                )
                .foregroundStyle(.white.opacity(TextOpacity.primary))
            }
        }
        .accessibilityIdentifier(identifier)
    }

    /// Only a REAL content conflict on an ISSUE-linked PR with a recorded
    /// branch gets the recovery run (EXP-533/734).
    private var canFixConflicts: Bool {
        guard case .issue = target else { return false }
        return steerEnabled
            && mergeFailure?.isConflict == true
            && !(issue?.branch ?? "").isEmpty
    }

    /// EXP-1145: a member of an open PR stack asks first; anything else
    /// confirms the plain merge.
    private func requestMerge() {
        if case let .issue(issueId) = target,
           let row = issue?.id == issueId ? issue : prIssues.first(where: { $0.id == issueId }),
           let choice = PrStack.stackMergeChoice(row, issues: prIssues) {
            stackChoice = choice
        } else {
            showMergeConfirm = true
        }
    }

    /// "Merge this pull request": the bottom member merges plainly, any other
    /// one lands the chain bottom-up THROUGH itself.
    private func mergeThis(_ choice: PrStack.StackMergeChoice) {
        guard choice.mergeThisUsesStack, case let .issue(issueId) = target else {
            merge()
            return
        }
        mergeStack(issueId: issueId)
    }

    /// No local surgery on success: the server ends the run and flips
    /// `pr_state`, and the pill leaves when that syncs back.
    private func merge() {
        state.failure = nil
        state.merging = true
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
                state.failure = failure
                toaster.error(failure.message)
            }
            state.merging = false
        }
    }

    /// EXP-1145: the stack merge through `issueId` (the top for Merge stack,
    /// the member itself for Merge this). A refusal toasts the server's
    /// message and never swaps the pill: the member that stopped the chain
    /// may not be this pull request.
    private func mergeStack(issueId: String) {
        state.failure = nil
        state.merging = true
        Task {
            do {
                try await deps.issuesApi.mergePr(
                    accountId: accountId, issueId: issueId, mergeStack: true
                )
            } catch {
                toaster.error(MergeFailure(error: error, stackMerge: true).message)
            }
            state.merging = false
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

/// EXP-1154: what every face's Merge capsule shares — the merge in flight and
/// the last refusal (a REAL conflict swaps Merge for Fix conflicts). The
/// screen resets it when the merge target changes.
struct WorkMergeState: Equatable {
    var merging = false
    var failure: MergeFailure?
}
