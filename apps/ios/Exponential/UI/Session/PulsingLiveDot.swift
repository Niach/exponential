import ExpUI
import SwiftUI

/// The live-session pulse: a solid green core with an expanding, fading ring —
/// the "Coding now" green, animated. Static under Reduce Motion. Shared by
/// `SessionStateDot`, the session rows and the Agents tab.
struct PulsingLiveDot: View {
    /// The disc's diameter. 9 on its own; the coding-now badge passes the
    /// pill's own `dotSize` so the live dot and the parked states' static dot
    /// are the same mark.
    var size: CGFloat = 9

    @Environment(\.motion) private var motion
    @State private var pulsing = false

    var body: some View {
        Circle()
            .fill(DesignTokens.Semantic.green)
            .frame(width: size, height: size)
            .overlay(
                Circle()
                    .stroke(DesignTokens.Semantic.green.opacity(0.6), lineWidth: 2)
                    .scaleEffect(pulsing ? 2.2 : 1.0)
                    .opacity(pulsing ? 0 : 0.8)
            )
            .onAppear {
                // EXP-523: the period and the outward-only curve stay this
                // indicator's own; only the Reduce Motion decision moves into
                // the shared helper. The guard stays because `pulsing` drives
                // the ring's resting scale/opacity too — flipping it with no
                // animation would snap the ring to scale 2.2 at opacity 0,
                // i.e. delete it, instead of leaving a static ring.
                guard !motion.reduceMotion else { return }
                withAnimation(
                    motion.pulse(duration: 1.4, autoreverses: false, curve: .easeOut)
                ) {
                    pulsing = true
                }
            }
    }
}
