import ExpCore
import ExpUI
import SwiftUI

/// EXP-874: the ONE live-run row, Android's `AgentSessionsList` row as the
/// reference — the Agent page's "Running" list and an action's "Runs" (live
/// runs; ended ones stay `EndedRunRow`).
///
/// EXP-1208: [run mark][fold chevron, parents only][text], ×4 — the shared
/// `AgentRunMark` (never a dot) at the row's base inset on every row, so a
/// parent's mark lines up with a standalone row's and a child's connector
/// elbow ends at its mark; the sub-lines align under the title.
///
/// Line 1 is `SessionRowTitle` (identifier for issue runs only, title);
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
    @ViewBuilder let footer: () -> Footer

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .center, spacing: SessionRowLead.gap) {
                runMark
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

    /// EXP-1208: the row's lead — the run's mark (Claude's spark while it
    /// works, a state badge when parked, the bare mark while paused).
    private var runMark: some View {
        let paused = device.isPaused(state, status: session.status)
        let working = CodingSessionDisplayState.working(
            status: session.status, state: state, paused: paused, live: true
        )
        return AgentRunMark(
            agent: session.agent,
            state: runningRowMarkState(state, paused: paused, working: working),
            badgeSize: SessionRowLead.badgeSize
        )
        .frame(width: SessionRowLead.markSize, height: SessionRowLead.markSize)
        .accessibilityHidden(true)
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
                .frame(width: SessionRowLead.markSize, height: GlassTokens.controlSize)
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
        let paused = device.isPaused(state, status: session.status)
        return HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                SessionRowTitle(identifier: identifier, title: title)
                // EXP-850 §8: the device-written caption, only on a live row
                // (web `sessionAgentCaption`: never on an ended row).
                if session.status != DomainContract.codingSessionStatusEnded,
                   let caption = session.agentCaption, !caption.isEmpty {
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
                ))
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
        onToggle: (() -> Void)? = nil
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
            footer: { EmptyView() }
        )
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
    case .working: return started.isEmpty ? device : "\(device) · started \(started)"
    }
}

/// The status line's tint (fixture `statusTone`): needs input amber, review
/// green, done blue; a paused or working row reads secondary (muted).
func sessionStatusLineColor(state: CodingSessionDisplayState, paused: Bool) -> Color {
    if paused { return .white.opacity(TextOpacity.secondary) }
    switch state {
    case .needsInput, .review, .done: return sessionStateColor(state)
    case .working: return .white.opacity(TextOpacity.secondary)
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
