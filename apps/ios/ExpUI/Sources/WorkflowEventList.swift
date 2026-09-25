import ExpCore
import SwiftUI

/// EXP-1082 (workflow contract): a workflow's EVENT LOG — the synced
/// `workflow_events` rows (the `workflow-events` shape), newest first, each a
/// kind glyph + the server's sentence + a relative time. STUB: renders
/// nothing yet; EXP-1068 fills the list.
public struct WorkflowEventList: View {
    public let events: [WorkflowEventEntity]

    public init(events: [WorkflowEventEntity]) {
        self.events = events
    }

    public var body: some View {
        VStack(spacing: 0) {
            EmptyView()
        }
    }
}
