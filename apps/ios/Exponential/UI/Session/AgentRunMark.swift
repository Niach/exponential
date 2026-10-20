import ExpCore
import ExpUI
import SwiftUI

// EXP-1184: the agent's WORKING mark and its resting brand mark, ×4 (web
// `@exp/ui` `AgentWorkingMark`, desktop + Android twins).

/// Claude's own "writing" spark: 8 hand-cut frames
/// (`agent-claude-writing-{0..7}` beside `agent-claude`), 90 ms each with HARD
/// cuts, looping. Under Reduce Motion it is the static `agent-claude` mark.
struct ClaudeSparkSpinner: View {
    static let frameCount = 8
    static let frameSeconds: TimeInterval = 0.09

    @Environment(\.motion) private var motion

    var body: some View {
        if motion.reduceMotion {
            Image("agent-claude")
                .resizable()
                .scaledToFit()
        } else {
            // EXP-1358: ONE schedule for every spark on screen — a fixed
            // epoch, so N busy rows tick in phase (N × 11 Hz out-of-phase
            // timelines each re-ran their row's body on its own beat).
            TimelineView(.periodic(from: Self.epoch, by: Self.frameSeconds)) { context in
                Image("agent-claude-writing-\(Self.frame(at: context.date))")
                    .resizable()
                    .scaledToFit()
            }
        }
    }

    /// The shared schedule's origin (any fixed instant in the past).
    static let epoch = Date(timeIntervalSinceReferenceDate: 0)

    static func frame(at date: Date) -> Int {
        Int(date.timeIntervalSinceReferenceDate / frameSeconds) % frameCount
    }
}

/// Whether a run's agent id is claude: a row without one is a claude run
/// (web `AgentBrandMark`'s rule).
func agentIsClaude(_ agent: String?) -> Bool {
    let id = (agent ?? "").trimmingCharacters(in: .whitespaces).lowercased()
    return id.isEmpty || id == "claude"
}

/// The agent at work: Claude's spark, or (an agent with no working art of its
/// own) its brand mark with the EXP-850 beat.
struct AgentWorkingMark: View {
    let agent: String?

    var body: some View {
        if agentIsClaude(agent) {
            ClaudeSparkSpinner()
        } else {
            BeatingAgentMark(agent: agent, beating: true)
        }
    }
}

/// The agent's brand mark (EXP-849: never a bare `Image("agent-…")`), with
/// the EXP-850 working beat — opacity 0.4 ↔ 1 over 1.4s — while `beating`;
/// steady under Reduce Motion.
struct BeatingAgentMark: View {
    let agent: String?
    let beating: Bool

    var body: some View {
        // A beating flip remounts the beat, so a stopped turn rests at full
        // opacity instead of freezing mid-cycle.
        Beat(agent: agent, beating: beating).id(beating)
    }

    private struct Beat: View {
        let agent: String?
        let beating: Bool

        @Environment(\.motion) private var motion
        @State private var dimmed = false

        var body: some View {
            mark
                .opacity(dimmed ? 0.4 : 1)
                .onAppear {
                    // EXP-1243: an ambient loop rides `motion.pulse` (nil under
                    // Reduce Motion: the mark stays still).
                    guard beating, let beat = motion.pulse(duration: 1.4) else { return }
                    withAnimation(beat) {
                        dimmed = true
                    }
                }
        }

        @ViewBuilder
        private var mark: some View {
            // A missing agent is a claude run (web parity); an id outside the
            // contract draws the agents glyph (`AgentBrandMark`). EXP-1312: a
            // template mark (codex, the glyph) takes the foreground, like web's
            // `currentColor` and Android's content colour.
            if let image = AgentBrandMark.image(agentIsClaude(agent) ? "claude" : agent ?? "claude") {
                image
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.white)
            } else {
                AppIcon(AppIcons.settingsAgents, size: DetailChrome.faceMark)
                    .foregroundStyle(.white)
            }
        }
    }
}
