import ExpCore
import ExpUI
import SwiftUI

// EXP-1184: the agent's WORKING mark and a live run's state mark, ×4 (web
// `@exp/ui` `AgentWorkingMark`/`AgentRunMark`, desktop + Android twins). The
// state they read is `CodingSessionDisplayState`, the
// `session-display.json` rule.

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
            TimelineView(.periodic(from: .now, by: Self.frameSeconds)) { context in
                Image("agent-claude-writing-\(Self.frame(at: context.date))")
                    .resizable()
                    .scaledToFit()
            }
        }
    }

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

/// A LIVE run's mark — wherever a run is named by its agent (the Work
/// screen's Run tab) it reads the same: the working mark while the agent
/// works, else its brand mark with a small state badge (amber: wants you,
/// green: PR open, blue: done). No `state` = a paused run, the bare mark. The caller sizes it.
struct AgentRunMark: View {
    let agent: String?
    let state: CodingSessionDisplayState?
    var badgeSize: CGFloat = 6

    var body: some View {
        mark
            .overlay(alignment: .topTrailing) {
                if let badge {
                    Circle()
                        .fill(badge)
                        .frame(width: badgeSize, height: badgeSize)
                        .offset(x: badgeSize / 3, y: -badgeSize / 3)
                }
            }
    }

    @ViewBuilder
    private var mark: some View {
        if state == .working {
            AgentWorkingMark(agent: agent)
        } else {
            BeatingAgentMark(agent: agent, beating: false)
        }
    }

    private var badge: Color? {
        guard let state, state != .working else { return nil }
        return sessionStateColor(state)
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
                    guard beating, !motion.reduceMotion else { return }
                    withAnimation(.easeInOut(duration: 1.4).repeatForever(autoreverses: true)) {
                        dimmed = true
                    }
                }
        }

        @ViewBuilder
        private var mark: some View {
            // A missing agent is a claude run (web parity); an id outside the
            // contract draws the agents glyph (`AgentBrandMark`).
            if let image = AgentBrandMark.image(agentIsClaude(agent) ? "claude" : agent ?? "claude") {
                image
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            } else {
                AppIcon(AppIcons.settingsAgents, size: DetailChrome.faceMark)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
    }
}
