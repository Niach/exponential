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
struct WorkMergePill: View {
    let pullRequest: PullRequestEntity
    /// The issues the PR links (`pull_request_issues`): the confirm names a
    /// batch's count, and an issue-less PR says so.
    let issueCount: Int
    /// Non-nil = `pullRequest` sits in an open stack: Merge lands it all.
    var stackConfirm: PullRequestStack.Confirm?
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

    private var label: String {
        stackConfirm?.title ?? DomainContract.diffUiMergePr
    }

    /// The plain merge confirm, or (EXP-1248) THE stack confirm, its copy
    /// byte-locked by `stack-merge-choice.json` `confirm`.
    private var confirmCard: GlassAlert<EmptyView> {
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
            case .capsule: pill
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

    private var circle: some View {
        FloatingBarCircle(
            accessibilityLabel: label,
            enabled: !merging,
            action: { showMergeConfirm = true }
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

    /// S-3: the header's action slot (the header Stop's shape: a plain
    /// toolbar button, the merge glyph before its label).
    private var toolbarButton: some View {
        Button { showMergeConfirm = true } label: {
            HStack(spacing: 6) {
                if merging {
                    ProgressView().controlSize(.small).tint(.white)
                } else {
                    AppIcon(AppIcons.prMerged, size: AppIcon.Size.small, weight: .medium)
                }
                Text(label)
                    .fontWeight(.medium)
                    .lineLimit(1)
            }
            .foregroundStyle(.white.opacity(TextOpacity.primary))
        }
        .disabled(merging)
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
