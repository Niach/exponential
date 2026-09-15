import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the Work screen's nav-bar title — IDENTICAL across faces, so it
/// never jumps: a small session-state dot (live / needs input; none without a
/// live run) beside the issue identifier, or the session title (Chat / the
/// action name / Batch run) for an issue-less run. No second caption line.
struct WorkTitle: View {
    let text: String
    /// Nil = no dot.
    let tone: SessionDotTone?
    /// EXP-848: pulse only while the agent is inside a turn.
    let pulsing: Bool

    var body: some View {
        HStack(spacing: 6) {
            if let tone {
                SessionStateDot(tone: tone, pulsing: pulsing, size: 8)
            }
            Text(text)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("work-title")
    }
}
