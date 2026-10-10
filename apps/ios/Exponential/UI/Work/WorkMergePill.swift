import ExpCore
import ExpUI
import SwiftUI

/// EXP-1150: the Work screen's ONE Merge PR. EXP-1154: back on the FLOATING
/// BOTTOM BAR as the SOLID WHITE capsule (`FloatingBarSolidPill`, hugging its
/// label, 52pt: the EXP-916 Reviews look), on every face — the centre of the
/// Changes bar's cluster `[files] [Merge PR]` and alone on the Results bar.
/// EXP-1191: the Issue and Run bars wear it as a 52pt glass CIRCLE (`style:
/// .circle`, the merge glyph alone) right of their centre capsule, the
/// floating capsule above them retired. EXP-1248: ONE merge control — it
/// reads "Merge stack" on a member of a linear open stack and asks ONE confirm
/// listing the pull requests that land (`PrStack.stackMergeConfirm`), else
/// the plain merge confirm. EXP-1233: a merge the
/// server refused on a REAL content conflict OPENS the Agent page composer on
/// the Fix merge conflicts builtin at once (this PR picked, the refusal
/// flagged) — the control never swaps to "Fix conflicts" any more, so a
/// conflict resolved elsewhere is one tap away. Every other refusal toasts.
/// The host resolves WHAT it merges (`target`):
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
    /// The issue's TEAM: the Work screen can sit on a non-active team
    /// (Reviews lists every team), and the recovery run's composer aligns
    /// its pools to this before it builds, else it cannot pick the PR.
    var teamId: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    /// EXP-1154: the merge in flight is the SCREEN's (`WorkMergeState`): each
    /// face mounts its own capsule, and all of them spin together.
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
    @State private var stackConfirm: PrStack.StackMergeConfirm?

    private var merging: Bool { state.merging }

    var body: some View {
        Group {
            switch style {
            case .capsule: pill
            case .circle: circle
            }
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4. Each
        // presentation on a zero-size node of its own (EXP-240): stacked on
        // one node SwiftUI drops the second.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showMergeConfirm) {
                    GlassAlert(prompt: mergePrompt, handlers: ["merge": { merge() }])
                }
        }
        // EXP-1248: the ONE stack confirm, its copy byte-locked by
        // `stack-merge-choice.json` `confirm`.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .stackMergeConfirmAlert($stackConfirm) { confirm in
                    mergeStack(issueId: confirm.issueId)
                }
        }
    }

    /// The `.issue` target's synced row.
    private var targetRow: IssueEntity? {
        guard case let .issue(issueId) = target else { return nil }
        return issue?.id == issueId ? issue : prIssues.first { $0.id == issueId }
    }

    /// EXP-1248: the control's Merge stack confirm, nil off a linear open
    /// stack.
    private var stackMerge: PrStack.StackMergeConfirm? {
        targetRow.flatMap { PrStack.stackMergeConfirm($0, issues: prIssues, mode: .stack) }
    }

    /// "Merge stack" on an open-stack member, else "Merge PR".
    private var label: String {
        stackMerge == nil ? DomainContract.diffUiMergePr : DomainContract.diffUiMergeStack
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
        FloatingBarSolidPill(
            accessibilityLabel: label,
            enabled: !merging,
            action: { requestMerge() }
        ) {
            if merging {
                ProgressView().controlSize(.small).tint(.black.opacity(0.6))
            } else {
                AppIcon(AppIcons.prMerged, size: FloatingBarTokens.glyph, weight: .medium)
            }
            Text(label)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .fixedSize()
        .accessibilityIdentifier(identifier)
    }

    /// EXP-1191: the bar circle — the SAME chrome as the bar's other circles,
    /// the merge glyph alone, a spinner while the merge is in flight.
    private var circle: some View {
        FloatingBarCircle(
            accessibilityLabel: label,
            enabled: !merging,
            action: { requestMerge() }
        ) {
            if merging {
                ProgressView().controlSize(.small).tint(.white)
            } else {
                AppIcon(AppIcons.prMerged, size: FloatingBarTokens.glyph, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
            }
        }
        .accessibilityIdentifier(identifier)
    }

    /// Only a REAL content conflict on an ISSUE-linked PR with a recorded
    /// branch, with the relay on, gets the recovery run (EXP-533/734); the
    /// builtin takes a representative ISSUE, so a run's own PR never does.
    private func canFixConflicts(_ failure: MergeFailure) -> Bool {
        guard case .issue = target else { return false }
        return steerEnabled
            && failure.isConflict
            && !(issue?.branch ?? "").isEmpty
    }

    /// EXP-1248: a member of an open PR stack asks the ONE stack confirm;
    /// anything else confirms the plain merge.
    private func requestMerge() {
        if let confirm = stackMerge {
            stackConfirm = confirm
        } else {
            showMergeConfirm = true
        }
    }

    /// No local surgery on success: the server ends the run and flips
    /// `pr_state`, and the pill leaves when that syncs back.
    private func merge() {
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
                // EXP-1233: a real conflict opens the recovery run's composer
                // at once (no toast: the composer's card says why it is up);
                // every other refusal toasts.
                if canFixConflicts(failure) {
                    openFixConflicts()
                } else {
                    toaster.error(failure.message)
                }
            }
            state.merging = false
        }
    }

    /// EXP-1248: ONE merge through `issueId` (the top for Merge stack). A
    /// refusal toasts the server's message and never opens the recovery run:
    /// the member that stopped the chain may not be this pull request.
    private func mergeStack(issueId: String) {
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
    /// EXP-1233: `conflict` flags the refusal, so the card says why.
    private func openFixConflicts() {
        guard case let .issue(issueId) = target else { return }
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: issueId,
                teamId: teamId,
                conflict: true
            )
        ))
    }
}

/// EXP-1154: what every face's Merge capsule shares — the merge in flight.
/// The screen resets it when the merge target changes.
struct WorkMergeState: Equatable {
    var merging = false
}

extension View {
    /// EXP-1248: the ONE stack merge confirm (`PrStack.stackMergeConfirm`):
    /// the title is the primary button, the body lists what lands; Cancel
    /// beside it. `onConfirm` sends `mergePr({issueId, mergeStack: true})`.
    func stackMergeConfirmAlert(
        _ confirm: Binding<PrStack.StackMergeConfirm?>,
        onConfirm: @escaping (PrStack.StackMergeConfirm) -> Void
    ) -> some View {
        glassAlert(item: confirm) { confirm in
            GlassAlert(
                title: confirm.title,
                message: confirm.body,
                actions: [
                    GlassAlertAction(PrStack.stackConfirmCancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                    GlassAlertAction(confirm.title, role: .primary, id: "merge-stack") {
                        onConfirm(confirm)
                    },
                ]
            )
        }
    }
}

/// EXP-1248: "Merge through here" off a stack row (the Guide's Stack card,
/// a long-press): the ONE confirm in `through` mode, then ONE merge through
/// that member. The merge in flight is the screen's (`WorkMergeState`).
struct StackMergeThroughHost: ViewModifier {
    @Binding var request: PrStack.StackMergeConfirm?
    @Binding var state: WorkMergeState

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    func body(content: Content) -> some View {
        content.background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .stackMergeConfirmAlert($request) { confirm in
                    state.merging = true
                    Task {
                        do {
                            try await deps.issuesApi.mergePr(
                                accountId: accountId, issueId: confirm.issueId, mergeStack: true
                            )
                        } catch {
                            toaster.error(MergeFailure(error: error, stackMerge: true).message)
                        }
                        state.merging = false
                    }
                }
        }
    }
}
