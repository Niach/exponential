import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the ONE session state dot — the Work screen's nav-bar title dot,
/// the face switcher's badge, and the issue bar's run circle used to draw
/// their own. Tone follows `SessionDotTone` (web `lib/session-dot.ts`, the
/// ×4 table): running / review green, needs-input amber, done blue, muted
/// neutral. It pulses ONLY while `pulsing` (the agent inside a turn, EXP-848)
/// and the tone is `running`.
struct SessionStateDot: View {
    let tone: SessionDotTone
    var pulsing: Bool = false
    var size: CGFloat = 9

    var body: some View {
        if pulsing, tone == .running {
            PulsingLiveDot(size: size)
        } else {
            Circle()
                .fill(Self.color(tone))
                .frame(width: size, height: size)
        }
    }

    static func color(_ tone: SessionDotTone) -> Color {
        switch tone {
        case .running, .review: DesignTokens.Semantic.green
        case .needsInput: DesignTokens.Semantic.yellow
        case .done: DesignTokens.Semantic.blue
        case .muted: DesignTokens.Semantic.neutral
        }
    }

    /// A synced row's display state as a dot tone.
    static func tone(of state: CodingSessionDisplayState) -> SessionDotTone {
        switch state {
        case .running: .running
        case .needsInput: .needsInput
        case .review: .review
        case .done: .done
        }
    }
}
