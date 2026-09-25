import ExpCore
import ExpUI
import SwiftUI

/// EXP-874: the ONE live-run row, Android's `AgentSessionsList` row as the
/// reference — the Agent page's "Running" list and Automations' "Recent
/// automated runs" (live runs; ended ones stay `EndedRunRow`).
///
/// Line 1 is `SessionRowTitle` (dot, identifier for issue runs only, title);
/// then the device-written caption on a live run, the status line
/// (`sessionStatusLine`), and the usage wall on its OWN line. EXP-893
/// dropped the trailing circles: a row only OPENS the run. The footer sits
/// under the row inside the same flat band.
struct RunningSessionRow<Footer: View>: View {
    let session: CodingSessionEntity
    /// Nil for a batch/action/chat run — the identifier is then hidden.
    let identifier: String?
    let title: String
    let state: CodingSessionDisplayState
    let device: SessionDevicePresentation
    let open: RunningSessionRowOpen
    /// EXP-897: this row has child runs nested under it, so it carries the
    /// fold chevron. Defaulted off — most rows are leaves.
    var expandable: Bool = false
    var expanded: Bool = true
    var onToggle: (() -> Void)?
    /// EXP-1068: the workflow marks (needs-you dot, duplicate warning, a
    /// non-default account). Empty on every other surface.
    var marks = RunningSessionRowMarks()
    @ViewBuilder let footer: () -> Footer

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 6) {
                foldControl
                primary
                    .frame(minHeight: GlassTokens.controlSize)
            }
            footer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }

    /// EXP-897: the fold, in a PLAIN Button OUTSIDE the row's
    /// NavigationLink/Button label — a control nested in a link's label has its
    /// tap swallowed by the link (the Reviews rows' trap, the ×4 rule).
    @ViewBuilder
    private var foldControl: some View {
        if expandable {
            Button { onToggle?() } label: {
                AppIcon(
                    expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 12
                )
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(width: 14, height: GlassTokens.controlSize)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse child runs" : "Expand child runs")
            .accessibilityIdentifier("session-fold")
        }
    }

    @ViewBuilder
    private var primary: some View {
        switch open {
        case let .route(route):
            NavigationLink(value: route) { content }
                .buttonStyle(.plain)
        case let .action(action):
            Button(action: action) { content }
                .buttonStyle(.plain)
        case .none:
            content
        }
    }

    private var content: some View {
        // EXP-550: the host machine stopped heartbeating (lid closed) — the
        // run is PAUSED, not ended, and resumes when the machine returns.
        let paused = device.isPaused(state)
        return HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    // EXP-1068: an open question to a person — red, beside
                    // the state dot (the amber needs-input state stays).
                    if marks.needsYou {
                        Circle()
                            .fill(DesignTokens.Semantic.red)
                            .frame(width: 7, height: 7)
                            .accessibilityLabel("Needs you")
                    }
                    SessionRowTitle(
                        identifier: identifier,
                        title: title,
                        state: state,
                        paused: paused,
                        // EXP-848: the dot pulses on the device-written turn flag.
                        busy: session.agentBusy
                    )
                    if marks.duplicateLive {
                        AppIcon(AppIcons.uiWarning, size: 12)
                            .foregroundStyle(DesignTokens.Semantic.yellow)
                            .accessibilityLabel("Two live runs on this node")
                    }
                }
                // EXP-850 §8: the device-written caption, only on a live row.
                if state != .done, let caption = session.agentCaption, !caption.isEmpty {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                Text(sessionStatusLine(
                    state: state,
                    paused: paused,
                    device: device.displayLabel,
                    started: relativeWireDate(session.startedAt)
                ) + (marks.account.map { " · account \($0)" } ?? ""))
                .font(.caption)
                .foregroundStyle(sessionStatusLineColor(state: state, paused: paused))
                .lineLimit(1)
                .truncationMode(.tail)
                // EXP-804: its OWN line under the state — a walled run still
                // reads `running`.
                SessionBlockedBadge(blocked: session.blocked)
            }
            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
    }
}

extension RunningSessionRow where Footer == EmptyView {
    init(
        session: CodingSessionEntity,
        identifier: String?,
        title: String,
        state: CodingSessionDisplayState,
        device: SessionDevicePresentation,
        open: RunningSessionRowOpen,
        expandable: Bool = false,
        expanded: Bool = true,
        onToggle: (() -> Void)? = nil,
        marks: RunningSessionRowMarks = RunningSessionRowMarks()
    ) {
        self.init(
            session: session,
            identifier: identifier,
            title: title,
            state: state,
            device: device,
            open: open,
            expandable: expandable,
            expanded: expanded,
            onToggle: onToggle,
            marks: marks,
            footer: { EmptyView() }
        )
    }
}

/// EXP-1068: what a workflow member's row adds to the plain run row.
struct RunningSessionRowMarks {
    /// The run holds an open question to a person (`pendingQuestion`).
    var needsYou = false
    /// Two live author (or review) runs on this node.
    var duplicateLive = false
    /// The account label, only when it is not the machine's default.
    var account: String?
}

extension RunningSessionRowMarks {
    /// EXP-1068: a REVIEW chain's title, `Review r2 · approved`, off its
    /// node's synced `review_round` + latest `review`. Nil on every other
    /// row. Shared by the Agent page's lists and the workflow page's Runs
    /// face (EXP-1083), so both read the same caption.
    ///
    /// `nodeReviewRound` + `review` = the node row's synced `review_round` and
    /// `review` json (the tree context's `WorkflowNode` or the entity).
    static func reviewTitle(
        _ node: SessionTree.SessionNode?, nodeReviewRound: Int?, review: String?
    ) -> String? {
        guard let node, node.session.workflowRole == DomainContract.wfSessionRoleReview else {
            return nil
        }
        let latest = WorkflowNodeReview.parse(review)
        let verdict = SessionTree.reviewRoundVerdict(
            round: node.reviewRound,
            nodeReviewRound: nodeReviewRound,
            latestRound: latest?.round,
            latestVerdict: latest?.verdict
        )
        return SessionTree.reviewRowCaption(
            round: node.reviewRound,
            verdict: verdict,
            live: SessionTree.sessionRowIsLive(status: node.session.status)
        )
    }

    /// The run's account label when it is NOT the host machine's default for
    /// that agent (`launch_defaults.defaultAccount` when the agent is the
    /// machine's default agent, else its ambient `system` login). A host this
    /// phone has not synced shows any non-ambient account.
    static func nonDefaultAccount(_ session: CodingSessionEntity, devices: [SteerDevice]?) -> String? {
        guard let account = session.agentAccount, !account.isEmpty else { return nil }
        let device = session.deviceId.flatMap { id in
            devices?.first { $0.deviceId == id }
        }
        let system = AgentAccountsRows.systemProfileId
        let defaults = device?.launchDefaults
        let fallback = defaults?.defaultAccount.flatMap { $0.isEmpty ? nil : $0 } ?? system
        let machineDefault = (session.agent != nil && defaults?.defaultAgent == session.agent)
            ? fallback : system
        guard account != machineDefault else { return nil }
        let profile = session.agent.flatMap { agent in
            device?.agentAccounts?[agent]?.profiles?.first { $0.id == account }
        }
        let label = profile?.label ?? profile?.email
        if let label, !label.isEmpty { return label }
        return account == system ? "Default" : account
    }
}

/// Where a `RunningSessionRow`'s primary tap goes.
enum RunningSessionRowOpen {
    case route(AppRoute)
    case action(() -> Void)
    case none
}

/// The run row's status line, byte-equal to Android's: `Paused · macbook`,
/// `Needs input · macbook`, `Ready for review · macbook`, `Done · macbook`,
/// else `macbook · started 5m ago` (the device alone when the start time does
/// not parse).
func sessionStatusLine(
    state: CodingSessionDisplayState,
    paused: Bool,
    device: String,
    started: String
) -> String {
    if paused { return "Paused · \(device)" }
    switch state {
    case .needsInput: return "Needs input · \(device)"
    case .review: return "Ready for review · \(device)"
    case .done: return "Done · \(device)"
    case .running: return started.isEmpty ? device : "\(device) · started \(started)"
    }
}

/// The status line's tint: the parked states wear their dot color, a paused
/// or simply running row reads secondary.
func sessionStatusLineColor(state: CodingSessionDisplayState, paused: Bool) -> Color {
    if paused { return .white.opacity(TextOpacity.secondary) }
    switch state {
    case .needsInput, .review, .done: return sessionStateColor(state)
    case .running: return .white.opacity(TextOpacity.secondary)
    }
}

/// "5m ago" off a synced timestamp. Electric syncs timestamps as Postgres
/// text (space separator, hour-only offset), which `WireTimestamps` handles
/// (EXP-169); empty when it does not parse.
func relativeWireDate(_ s: String) -> String {
    guard let date = WireTimestamps.parse(s) else { return "" }
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .short
    return formatter.localizedString(for: date, relativeTo: Date())
}
