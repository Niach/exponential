import ExpCore
import ExpUI
import SwiftUI

/// EXP-1150/1154: the ONE Merge PR control — the SOLID WHITE capsule
/// (`FloatingBarSolidPill`, hugging its label, 52pt) or (EXP-1191) the
/// icon-only 52pt glass bar circle. EXP-1295: it merges a `pull_requests` row
/// through `pullRequests.merge`. EXP-1303: on a member of an open stack
/// (`stackConfirm`, Android `PullRequestStack`) it reads "Merge stack", asks
/// THE stack confirm and merges THROUGH the stack's top (`stack: true`). Every refusal toasts the server's words
/// (EXP-1391: the server also posts it into the thread's root, which takes it
/// over); every refusal, a conflict included, offers Retry.
/// The host mounts it only while the PR is open. S-3: `.toolbar` = the same
/// control as a header button (the thread header's action slot, left of `…`).
/// #EXP-1611 (pr-checks.json MERGE PILL, `PrChecks.mergePill` over the PR's
/// `checks_state`): pending = THE spinner in the glyph's place, still
/// enabled; failure = the PLAIN pill, the red `ci-failed` glyph, `Checks
/// failing`, and its tap asks prompts.json `merge-failing-pr` INSTEAD OF the
/// plain / stack confirm (Merge merges exactly as the pill would; the
/// person's merge never sends `force`).
struct WorkMergePill: View {
    let pullRequest: PullRequestEntity
    /// The issues the PR links (`pull_request_issues`): the confirm names a
    /// batch's count, and an issue-less PR says so.
    let issueCount: Int
    /// Non-nil = `pullRequest` sits in an open stack: Merge lands it all.
    var stackConfirm: PullRequestStack.Confirm?
    /// #EXP-1611: the PR's synced CI rows (the failing confirm names them).
    var checks: [PrChecks.Check] = []
    /// The merge in flight is the HOST's: every mounted capsule spins together.
    @Binding var state: WorkMergeState
    var identifier = "work-merge-pr"
    var style: Style = .capsule
    enum Style { case capsule, circle, toolbar }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var showMergeConfirm = false

    private var merging: Bool { state.merging }

    /// #EXP-1611: the pill over the PR's check state.
    private var view: PrChecks.MergePill {
        PrChecks.mergePill(state: pullRequest.checksState, stack: stackConfirm != nil)
    }

    private var label: String {
        view.confirm == .failing ? view.label : stackConfirm?.title ?? DomainContract.diffUiMergePr
    }

    /// The glyph slot: the merge in flight's spinner, THE spinner while CI
    /// runs, the red `ci-failed` while it fails, else the merge glyph.
    @ViewBuilder
    private func glyph(size: CGFloat, weight: Font.Weight? = .medium, spinnerTint: Color?, glyphColor: Color?) -> some View {
        if merging || view.glyph == .spinner {
            // THE spinner, dark on the white primary paint (a default-tinted
            // one vanished into it).
            InlineSpinner(size: size, color: spinnerTint ?? .white.opacity(TextOpacity.tertiary))
        } else if view.glyph == .ciFailed {
            AppIcon(AppIcons.ciFailed, size: size, weight: weight)
                .foregroundStyle(DesignTokens.Semantic.red)
        } else if let glyphColor {
            AppIcon(AppIcons.prMerged, size: size, weight: weight)
                .foregroundStyle(glyphColor)
        } else {
            AppIcon(AppIcons.prMerged, size: size, weight: weight)
        }
    }

    /// The plain merge confirm, or (EXP-1248) THE stack confirm, its copy
    /// byte-locked by `stack-merge-choice.json` `confirm`; (#EXP-1611) while
    /// checks fail, `merge-failing-pr` replaces both.
    private var confirmCard: GlassAlert<EmptyView> {
        if view.confirm == .failing {
            return GlassAlert(
                prompt: Prompts.MergeFailingPr.copy(
                    number: pullRequest.number, stack: stackConfirm != nil, body: PrChecks.failingBody(checks)
                ),
                handlers: ["merge": { merge() }]
            )
        }
        guard let stackConfirm else {
            return GlassAlert(prompt: mergePrompt, handlers: ["merge": { merge() }])
        }
        return GlassAlert(
            title: stackConfirm.title,
            message: stackConfirm.body,
            actions: [
                GlassAlertAction(PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                GlassAlertAction(stackConfirm.title, role: .primary, id: "merge-stack") { merge() },
            ]
        )
    }

    private var mergePrompt: PromptCopy {
        issueCount == 0
            ? Prompts.MergeRunPr.copy(number: pullRequest.number)
            : Prompts.MergeIssuePr.copy(number: pullRequest.number, issueCount: issueCount)
    }

    var body: some View {
        Group {
            switch style {
            // The failing pill is THE plain pill, never the solid one.
            case .capsule: if view.primary { pill } else { toolbarButton }
            case .circle: circle
            case .toolbar: toolbarButton
            }
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), on a zero-size
        // node of its own (EXP-240).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showMergeConfirm) { confirmCard }
        }
    }

    private var pill: some View {
        FloatingBarSolidPill(
            accessibilityLabel: label,
            enabled: !merging,
            action: { showMergeConfirm = true }
        ) {
            glyph(size: FloatingBarTokens.glyph, spinnerTint: .black.opacity(0.6), glyphColor: nil)
            Text(label)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .fixedSize()
        .accessibilityIdentifier(identifier)
    }

    private var circle: some View {
        FloatingBarCircle(
            accessibilityLabel: label,
            enabled: !merging,
            action: { showMergeConfirm = true }
        ) {
            glyph(size: FloatingBarTokens.glyph, spinnerTint: .white, glyphColor: .white.opacity(TextOpacity.primary))
        }
        .accessibilityIdentifier(identifier)
    }

    /// S-3: the header's action slot (the header Stop's shape: a plain
    /// toolbar button, the merge glyph before its label).
    /// EXP-1552: the thread header's Merge = THE md primary `GlassPill` ×4
    /// (web `PullRequestMergePill`, desktop `.primary()`, Android
    /// `MergePill(header = true)`).
    private var toolbarButton: some View {
        GlassPill(
            label,
            size: .md,
            mode: .action { showMergeConfirm = true },
            primary: view.primary,
            enabled: !merging
        ) {
            glyph(
                size: GlassPillSize.md.glyphSize, weight: nil,
                spinnerTint: view.primary && !merging ? .black.opacity(0.6) : nil, glyphColor: nil
            )
        }
        .fixedSize()
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
    }

    /// No local surgery on success: the `state` flip syncs back and the host
    /// unmounts the capsule.
    private func merge() {
        state.merging = true
        let pullRequestId = stackConfirm?.throughId ?? pullRequest.id
        let stack = stackConfirm != nil
        let title = FeedbackStates.Errors.title(verb: "merge", thing: stack ? "the stack" : "the PR")
        Task {
            do {
                let result = try await deps.pullRequestsApi.merge(
                    accountId: accountId,
                    pullRequestId: pullRequestId,
                    stack: stack ? true : nil
                )
                if !result.merged, result.queued != true {
                    toaster.error(title, description: result.error)
                }
            } catch {
                toaster.error(
                    title,
                    description: error.userFacingMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) {
                        Task { @MainActor in merge() }
                    }
                )
            }
            state.merging = false
        }
    }
}

/// EXP-1154: what every mounted Merge capsule shares — the merge in flight.
/// The host resets it when the PR changes.
struct WorkMergeState: Equatable {
    var merging = false
}
