import ExpCore
import ExpUI
import SwiftUI

/// EXP-1175: the Run face's ONE status row, ×4 (web `RunStatusRow`, desktop
/// `run_rows.rs`, Android `RunStatusRow`) — the agent's run mark as the
/// spinner, `WorkFaces.runRowCaption` in its tone, `AgentFeed.lastToolLine`
/// muted under it, and the Show work / Hide work switch on the right. It sits
/// over the thread AND the transcript; only the button label flips.
struct RunStatusRow: View {
    let agent: String?
    /// The mark's state (`runningRowMarkState`); ignored once `ended`.
    let markState: CodingSessionDisplayState?
    let ended: Bool
    let rowState: RunRowState
    /// The caption at `now` — re-derived once a second while working.
    let caption: (Date) -> RunRowCaption
    let toolLine: String?
    let showWork: Bool
    let onToggle: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: SessionRowLead.gap) {
            AgentRunMark(agent: agent, state: ended ? nil : markState, ended: ended)
                .frame(width: SessionRowLead.markSize, height: SessionRowLead.markSize)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                captionLine
                if let toolLine {
                    Text(toolLine)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Button(WorkFaces.showWorkLabel(showWork), action: onToggle)
                .buttonStyle(.plain)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .accessibilityIdentifier("run-show-work")
        }
        .padding(.horizontal, AgentSessionLayout.gutter)
        .padding(.vertical, 8)
        .transcriptColumnWidth()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("run-status-row")
    }

    /// Only a WORKING caption carries a ticking clock (the `WorkingIndicatorRow`
    /// recipe); every other state is a fixed line.
    @ViewBuilder
    private var captionLine: some View {
        if rowState == .working {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                captionText(caption(context.date))
            }
        } else {
            captionText(caption(Date()))
        }
    }

    private func captionText(_ line: RunRowCaption) -> some View {
        Text(line.text)
            .font(.caption)
            .foregroundStyle(runRowToneColor(line.tone))
            .lineLimit(1)
            .truncationMode(.tail)
    }
}

/// The caption tones in this app's palette — the session list row's tints.
func runRowToneColor(_ tone: RunRowTone) -> Color {
    switch tone {
    case .muted: .white.opacity(TextOpacity.secondary)
    case .amber: sessionStateColor(.needsInput)
    case .emerald: sessionStateColor(.review)
    case .sky: sessionStateColor(.done)
    }
}
