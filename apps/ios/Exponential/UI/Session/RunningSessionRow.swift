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
    @ViewBuilder let footer: () -> Footer

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            primary
                .frame(minHeight: GlassTokens.controlSize)
            footer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
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
                SessionRowTitle(
                    identifier: identifier,
                    title: title,
                    state: state,
                    paused: paused,
                    // EXP-848: the dot pulses on the device-written turn flag.
                    busy: session.agentBusy
                )
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
        open: RunningSessionRowOpen
    ) {
        self.init(
            session: session,
            identifier: identifier,
            title: title,
            state: state,
            device: device,
            open: open,
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
