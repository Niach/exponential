import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1710 (merge-train.json STATUS PILL, `MergeTrain.statusPill` over
/// the PR's train columns): THE header pill is a STATUS, no longer the
/// person's Merge. THE Pill md, mode action, NOT primary; its glyph by icon
/// concept (`spinner` = THE spinner), tone red tinting only the glyph; its
/// tap opens the existing Checks submenu (pr-checks.json CHECKS LIST; no
/// rows = readonly, no tap). THE ONLY ACTION: in `approval` mode a ready PR
/// reads `Needs approval` (PRIMARY, pr-merged), whose tap asks the plain
/// merge confirm (prompts.json merge-issue-pr / merge-run-pr) or (EXP-1303)
/// THE stack confirm, and Merge QUEUES it through `pullRequests.merge` (the
/// train lands it). Every refusal toasts the server's words with Retry.
/// `.capsule` / `.circle` = the phone's bottom Merge capsule, the same
/// status; `.toolbar` = the thread header's action slot, left of `…`.
struct WorkMergePill: View {
    let pullRequest: PullRequestEntity
    /// The issues the PR links (`pull_request_issues`): the confirm names a
    /// batch's count, and an issue-less PR says so.
    let issueCount: Int
    /// Non-nil = `pullRequest` sits in an open stack: approving queues it all.
    var stackConfirm: PullRequestStack.Confirm?
    /// #EXP-1611: the PR's synced CI rows (none = a readonly pill).
    var checks: [PrChecks.Check] = []
    /// `Testing with N`: the synced PRs sharing this PR's `merge_train_id`,
    /// this one included.
    var batchSize: Int?
    /// A member may approve; a viewer sees the status only.
    var canApprove = true
    /// The tap of a status pill: the host opens its Checks submenu.
    var onChecks: (() -> Void)?
    /// The merge in flight is the HOST's: every mounted capsule spins together.
    @Binding var state: WorkMergeState
    var identifier = MergeTrain.TestIds.statusPill
    var style: Style = .capsule
    enum Style { case capsule, circle, toolbar }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var showMergeConfirm = false

    private var merging: Bool { state.merging }

    /// THE view model, shared by every style.
    private var view: MergeTrain.StatusPill {
        Self.statusPill(
            pullRequest, stack: stackConfirm != nil, checks: checks.count, batchSize: batchSize, canApprove: canApprove
        )
    }

    /// The PR's own state wins (merged, draft), else its train state; a
    /// viewer who may not approve sees Needs approval as a plain status.
    /// The host mounts no pill on a closed PR (`MergeTrain.pillState` nil).
    static func statusPill(
        _ pullRequest: PullRequestEntity, stack: Bool, checks: Int, batchSize: Int?, canApprove: Bool
    ) -> MergeTrain.StatusPill {
        let pill = MergeTrain.statusPill(
            state: MergeTrain.pillState(prState: pullRequest.state, mergeState: pullRequest.mergeState),
            wait: pullRequest.mergeWait,
            position: pullRequest.mergePosition,
            batchSize: batchSize,
            holdReason: pullRequest.mergeHold,
            stack: stack,
            checks: checks
        )
        return MergeTrain.viewerPill(pill, canApprove: canApprove, checks: checks)
    }

    private var action: (() -> Void)? {
        switch view.tap {
        case .approve:
            guard canApprove, !merging else { return nil }
            return { showMergeConfirm = true }
        case .checks:
            return onChecks
        case nil:
            return nil
        }
    }

    private func glyph(size: CGFloat, weight: Font.Weight? = .medium, spinnerTint: Color?, glyphColor: Color?) -> some View {
        MergeStatusGlyph(
            view: view, merging: merging, size: size, weight: weight, spinnerTint: spinnerTint, glyphColor: glyphColor
        )
    }

    /// The plain merge confirm, or (EXP-1248) THE stack confirm, its copy
    /// byte-locked by `stack-merge-choice.json` `confirm`.
    private var confirmCard: GlassAlert<EmptyView> {
        Self.approveCard(number: pullRequest.number, issueCount: issueCount, stackConfirm: stackConfirm) { merge() }
    }

    /// #EXP-1710: THE Needs approval confirm, shared by the pill and the
    /// Needs you ready row's press: the plain merge confirm (prompts.json
    /// merge-issue-pr / merge-run-pr) or THE stack confirm.
    static func approveCard(
        number: Int, issueCount: Int, stackConfirm: PullRequestStack.Confirm?, onMerge: @escaping () -> Void
    ) -> GlassAlert<EmptyView> {
        guard let stackConfirm else {
            let prompt = issueCount == 0
                ? Prompts.MergeRunPr.copy(number: number)
                : Prompts.MergeIssuePr.copy(number: number, issueCount: issueCount)
            return GlassAlert(prompt: prompt, handlers: ["merge": onMerge])
        }
        return GlassAlert(
            title: stackConfirm.title,
            message: stackConfirm.body,
            actions: [
                GlassAlertAction(PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                GlassAlertAction(stackConfirm.title, role: .primary, id: "merge-stack") { onMerge() },
            ]
        )
    }

    var body: some View {
        Group {
            switch style {
            // A status is THE plain pill, never the solid one.
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
            accessibilityLabel: view.label,
            enabled: action != nil,
            action: { action?() }
        ) {
            glyph(size: FloatingBarTokens.glyph, spinnerTint: .black.opacity(0.6), glyphColor: nil)
            Text(view.label)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .fixedSize()
        .accessibilityIdentifier(identifier)
    }

    private var circle: some View {
        FloatingBarCircle(
            accessibilityLabel: view.label,
            enabled: action != nil,
            action: { action?() }
        ) {
            glyph(size: FloatingBarTokens.glyph, spinnerTint: .white, glyphColor: .white.opacity(TextOpacity.primary))
        }
        .accessibilityIdentifier(identifier)
    }

    /// S-3: the header's action slot. EXP-1552: THE md `GlassPill` ×4 (web
    /// `MergeStatusPill`, desktop, Android `MergeStatusPill`),
    /// primary only for `Needs approval`.
    private var toolbarButton: some View {
        MergeStatusPill(view: view, merging: merging, action: action, identifier: identifier)
    }

    /// Needs approval: `pullRequests.merge` QUEUES it (the train is on). No
    /// local surgery on success: the train columns sync back.
    private func merge() {
        state.merging = true
        MergeTrainActions(deps: deps, accountId: accountId, toaster: toaster).approve(
            pullRequestId: stackConfirm?.throughId ?? pullRequest.id,
            stack: stackConfirm != nil,
            onStart: { state.merging = true },
            onSettled: { state.merging = false }
        )
    }
}

/// #EXP-1710: THE status pill's paint (the header's md `GlassPill`), over a
/// `MergeTrain.StatusPill`: the thread header and the styleguide specimen.
struct MergeStatusPill: View {
    let view: MergeTrain.StatusPill
    var merging = false
    /// nil = readonly (no tap).
    let action: (() -> Void)?
    var identifier = MergeTrain.TestIds.statusPill

    var body: some View {
        GlassPill(
            view.label,
            size: .md,
            mode: action.map { tap in GlassPillMode.action { tap() } } ?? GlassPillMode.readonly,
            primary: view.primary,
            enabled: view.tap != .approve || !merging
        ) {
            MergeStatusGlyph(
                view: view, merging: merging, size: GlassPillSize.md.glyphSize, weight: nil,
                spinnerTint: view.primary && !merging ? .black.opacity(0.6) : nil, glyphColor: nil
            )
        }
        .fixedSize()
        .accessibilityLabel(view.label)
        .accessibilityIdentifier(identifier)
    }
}

/// The glyph slot: the merge in flight's spinner, THE spinner, else the
/// state's concept glyph (red tone = the glyph alone).
struct MergeStatusGlyph: View {
    let view: MergeTrain.StatusPill
    let merging: Bool
    let size: CGFloat
    var weight: Font.Weight? = .medium
    var spinnerTint: Color?
    var glyphColor: Color?

    var body: some View {
        if merging || view.glyph == .spinner {
            // THE spinner, dark on the white primary paint (a default-tinted
            // one vanished into it).
            InlineSpinner(size: size, color: spinnerTint ?? .white.opacity(TextOpacity.tertiary))
        } else {
            let icon = AppIcons.conceptOrName(view.glyph.rawValue) ?? AppIcons.prMerged
            if view.tone == .red {
                AppIcon(icon, size: size, weight: weight)
                    .foregroundStyle(DesignTokens.Semantic.red)
            } else if let glyphColor {
                AppIcon(icon, size: size, weight: weight)
                    .foregroundStyle(glyphColor)
            } else {
                AppIcon(icon, size: size, weight: weight)
            }
        }
    }
}

/// EXP-1154: what every mounted Merge capsule shares — the merge in flight.
/// The host resets it when the PR changes.
struct WorkMergeState: Equatable {
    var merging = false
}

/// #EXP-1710 (merge-train.json MENU / NEEDS YOU): the train's person
/// actions, shared by the thread's `…` and the Needs you stuck row. Merge
/// now = `pullRequests.mergeNow` (owners, after THE `merge-now-pr` alert);
/// Hold / Resume auto-merge and Retry = `pullRequests.hold`. A failure = a
/// toast; the state flips land over the shapes.
@MainActor
struct MergeTrainActions {
    let deps: AppDependencies
    let accountId: String
    let toaster: Toaster

    /// Needs approval (the pill, the Needs you ready row): `pullRequests.merge`
    /// QUEUES it (the stack's top when `stack`); a refusal toasts the
    /// server's words, a failure offers Retry.
    func approve(
        pullRequestId: String, stack: Bool,
        onStart: @escaping @MainActor () -> Void = {}, onSettled: @escaping @MainActor () -> Void = {}
    ) {
        let api = deps.pullRequestsApi
        let accountId = accountId
        let toaster = toaster
        let title = FeedbackStates.Errors.title(verb: "merge", thing: stack ? "the stack" : "the PR")
        Task {
            do {
                let result = try await api.merge(
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
                        Task { @MainActor in
                            onStart()
                            approve(pullRequestId: pullRequestId, stack: stack, onStart: onStart, onSettled: onSettled)
                        }
                    }
                )
            }
            onSettled()
        }
    }

    func mergeNow(pullRequestId: String) {
        let api = deps.pullRequestsApi
        let accountId = accountId
        let toaster = toaster
        Task {
            do {
                try await api.mergeNow(accountId: accountId, pullRequestId: pullRequestId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "merge", thing: "the PR"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// `hold` = Hold auto-merge; false = Resume auto-merge / Retry.
    func hold(pullRequestId: String, hold: Bool) {
        let api = deps.pullRequestsApi
        let accountId = accountId
        let toaster = toaster
        Task {
            do {
                try await api.hold(accountId: accountId, pullRequestId: pullRequestId, hold: hold)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: hold ? "hold" : "resume", thing: "auto-merge"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    func close(pullRequestId: String) {
        let api = deps.pullRequestsApi
        let accountId = accountId
        let toaster = toaster
        Task {
            do {
                try await api.close(accountId: accountId, pullRequestId: pullRequestId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "close", thing: "the PR"),
                    description: error.userFacingMessage
                )
            }
        }
    }
}

extension View {
    /// #EXP-1710: THE shared `merge-now-pr` alert card (Merge destructive,
    /// Cancel focused; `titleStack` on an open-stack member).
    func mergeNowConfirm(
        target: Binding<MergeNowTarget?>, onConfirm: @escaping (MergeNowTarget) -> Void
    ) -> some View {
        background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: target) { item in
                    GlassAlert(
                        prompt: Prompts.MergeNowPr.copy(number: item.number, stack: item.stack),
                        handlers: ["merge": { onConfirm(item) }]
                    )
                }
        }
    }
}

/// #EXP-1710: the PR a Merge now alert asks about.
struct MergeNowTarget: Identifiable, Equatable {
    let pullRequestId: String
    let number: Int
    /// The PR sits in an open stack (the alert's `titleStack`).
    let stack: Bool
    var id: String { pullRequestId }

    /// THE one rule (the thread `…` and the Needs you stuck row): `titleStack`
    /// while the PR sits in an open stack among `pool`.
    init(_ pullRequest: PullRequestEntity, pool: [PullRequestEntity]) {
        self.init(
            pullRequestId: pullRequest.id,
            number: pullRequest.number,
            stack: pullRequest.isOpen && PullRequestStack.stackConfirm(pullRequest, in: pool) != nil
        )
    }

    init(pullRequestId: String, number: Int, stack: Bool) {
        self.pullRequestId = pullRequestId
        self.number = number
        self.stack = stack
    }
}

/// #EXP-1710: the PR a Needs approval confirm asks about (the pill's own
/// confirm, raised from a Needs you ready row).
struct NeedsApprovalTarget: Identifiable, Equatable {
    let pullRequestId: String
    let number: Int
    let issueCount: Int
    /// Non-nil = it sits in an open stack: approving queues it all.
    let stackConfirm: PullRequestStack.Confirm?
    var id: String { pullRequestId }
}

extension View {
    /// #EXP-1710: THE pill's Needs approval confirm over a target; Merge
    /// queues it, Cancel leaves it.
    func needsApprovalConfirm(
        target: Binding<NeedsApprovalTarget?>, onConfirm: @escaping (NeedsApprovalTarget) -> Void
    ) -> some View {
        background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: target) { item in
                    WorkMergePill.approveCard(
                        number: item.number, issueCount: item.issueCount, stackConfirm: item.stackConfirm
                    ) { onConfirm(item) }
                }
        }
    }
}
