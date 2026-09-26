import ExpCore
import SwiftUI

/// EXP-1082 → EXP-1068: a workflow's EVENT LOG — the synced `workflow_events`
/// rows (the `workflow-events` shape), newest first. Each row = the kind's
/// concept glyph, the node's identifier as a mono lead-in (when `nodeLabel`
/// resolves it), the server's sentence on one line and the local `HH:mm`. A
/// row naming a run opens it through `onOpenSession`. Flat rows; an empty log
/// draws nothing.
public struct WorkflowEventList: View {
    public let events: [WorkflowEventEntity]
    /// A node id → its identifier (`EXP-14`); nil = no lead-in.
    public let nodeLabel: ((String) -> String?)?
    /// Opens the run a row names; nil = rows are not tappable.
    public let onOpenSession: ((String) -> Void)?

    public init(
        events: [WorkflowEventEntity],
        nodeLabel: ((String) -> String?)? = nil,
        onOpenSession: ((String) -> Void)? = nil
    ) {
        self.events = events
        self.nodeLabel = nodeLabel
        self.onOpenSession = onOpenSession
    }

    public var body: some View {
        VStack(spacing: 0) {
            ForEach(Self.ordered(events), id: \.id) { event in
                row(event)
            }
        }
    }

    /// Newest first: `at` descending, then id descending.
    static func ordered(_ events: [WorkflowEventEntity]) -> [WorkflowEventEntity] {
        events.sorted { a, b in
            let (ta, tb) = (instant(a.at), instant(b.at))
            return ta != tb ? ta > tb : a.id > b.id
        }
    }

    private static func instant(_ value: String) -> TimeInterval {
        WireTimestamps.parse(value)?.timeIntervalSince1970 ?? 0
    }

    /// The concept glyph a kind (contract `wfEventKind`) wears.
    static func glyph(_ kind: String) -> String {
        switch kind {
        case DomainContract.wfEventKindNodeStarted: AppIcons.actionRun
        case DomainContract.wfEventKindRetrying: AppIcons.runResume
        case DomainContract.wfEventKindReviewStarted,
             DomainContract.wfEventKindReviewVerdict,
             DomainContract.wfEventKindReviewNoVerdict: AppIcons.codingInReview
        case DomainContract.wfEventKindLanded: AppIcons.uiCheck
        case DomainContract.wfEventKindCompleted: AppIcons.uiCheck
        case DomainContract.wfEventKindFinalPrOpened,
             DomainContract.wfEventKindFinalPrReopened: AppIcons.prOpen
        case DomainContract.wfEventKindFailed,
             DomainContract.wfEventKindGaveUp: AppIcons.uiWarning
        case DomainContract.wfEventKindAccountPicked,
             DomainContract.wfEventKindAccountSwitched: AppIcons.navAccount
        case DomainContract.wfEventKindWaitingReset: AppIcons.uiClock
        case DomainContract.wfEventKindQuestionAsked,
             DomainContract.wfEventKindQuestionAnswered: AppIcons.uiHelp
        case DomainContract.wfEventKindCancelled,
             DomainContract.wfEventKindSkipped: AppIcons.uiClose
        default: AppIcons.uiInfo
        }
    }

    /// The kinds that read as a warning (amber glyph), web `WARNING_KINDS`.
    static func isWarning(_ kind: String) -> Bool {
        switch kind {
        case DomainContract.wfEventKindFailed,
             DomainContract.wfEventKindGaveUp,
             DomainContract.wfEventKindReviewNoVerdict,
             DomainContract.wfEventKindWaitingReset: true
        default: false
        }
    }

    private static let clock: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "HH:mm"
        return formatter
    }()

    /// The event's local `HH:mm`; empty when the stamp does not parse.
    static func time(_ at: String) -> String {
        WireTimestamps.parse(at).map { clock.string(from: $0) } ?? ""
    }

    @ViewBuilder
    private func row(_ event: WorkflowEventEntity) -> some View {
        if let sessionId = event.sessionId, !sessionId.isEmpty, let onOpenSession {
            Button { onOpenSession(sessionId) } label: { content(event) }
                .buttonStyle(.plain)
        } else {
            content(event)
        }
    }

    private func content(_ event: WorkflowEventEntity) -> some View {
        let label = event.nodeId.flatMap { id in nodeLabel?(id) }
        return HStack(spacing: 8) {
            AppIcon(Self.glyph(event.kind), size: 14)
                .foregroundStyle(
                    Self.isWarning(event.kind)
                        ? DesignTokens.Semantic.yellow : .white.opacity(TextOpacity.secondary)
                )
            if let label, !label.isEmpty {
                Text(label)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            Text(event.message)
                .font(.caption)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
            Text(Self.time(event.at))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .flatRow()
        .accessibilityIdentifier("workflow-event-\(event.kind)")
    }
}
