import Foundation

/// EXP-1275 / EXP-1271 / FEED-79 (EXP-1259 D8-D11): THE feedback-states
/// system — inline, pending, placeholder, the mark, blocking, errors — as the
/// pure constants and the mark's timeline every iOS view paints from. Locked
/// against `packages/domain-contract/fixtures/feedback-states.json` by
/// `FeedbackStatesTests`; the mark timeline is a port of the TS reference
/// `packages/domain-contract/src/feedback-mark.ts` (desktop
/// `ui::feedback::mark`, Android `components/FeedbackMark.kt`, web `@exp/ui`
/// `<FeedbackMark>`).
public enum FeedbackStates {
    public enum TestId {
        public static let spinner = "feedback-spinner"
        public static let pending = "feedback-pending"
        public static let skeleton = "feedback-skeleton"
        public static let mark = "feedback-mark"
        public static let blocking = "feedback-blocking"
        public static let starting = "composer-starting"
    }

    /// (1) INLINE: a 10pt spinner + a progressive word in a band or caption.
    public enum Inline {
        public static let spinner: Double = 10
        public static let gap: Double = 6
        public static let saving = "Saving"
        public static let saved = "Saved"
        public static let loading = "Loading"
    }

    /// (2) PENDING: the icon slot becomes the spinner, the label the
    /// progressive word, width fixed, and nothing changes before `delayMs`.
    public enum Pending {
        public static let delayMs = 150
        public static let spinner: Double = 12
        public static let start = "Starting"
        public static let connect = "Connecting"
        public static let save = "Saving"
        public static let delete = "Deleting"
        public static let send = "Sending"
        public static let merge = "Merging"
        public static let invite = "Inviting"
    }

    /// (3) PLACEHOLDER: skeleton rows in the real row geometry.
    public enum Placeholder {
        public static let rows = 3
        public static let barHeight: Double = 8
        public static let radius: Double = 4
        public static let dot: Double = 12
        public static let pulseMs = 1600
        public static let opacity: (low: Double, high: Double) = (0.45, 0.9)
        public static let reducedOpacity = 0.6
    }

    /// (4) THE MARK: the logo disc whose three cut stripes sweep in turn.
    public enum Mark {
        public static let loopMs = 1200
        public static let samples = 24
        /// The cut's width on the logo's 100×100 grid.
        public static let stripeWidth: Double = 6
        public static let inlineSize: Double = 16
        public static let statusSize: Double = 16
        public static let styleguideSize: Double = 40

        public struct Stripe: Equatable, Sendable {
            /// The cubic's four points (start, control 1, control 2, end) on the
            /// logo's 100×100 grid; the disc (r 50 at 50,50) clips it.
            public let cubic: [(x: Double, y: Double)]
            public let draw: (from: Double, to: Double)
            public let erase: (from: Double, to: Double)

            public static func == (a: Stripe, b: Stripe) -> Bool {
                a.cubic.map(\.x) == b.cubic.map(\.x) && a.cubic.map(\.y) == b.cubic.map(\.y)
                    && a.draw == b.draw && a.erase == b.erase
            }
        }

        /// The logo's three swept curves (`apps/desktop/assets/icons/logo.svg`)
        /// — the ONE copy the brand mark (`ExpLogoMark`) and the feedback mark
        /// both draw from. Index 0 = the lower-left stripe.
        public static let stripes: [Stripe] = [
            Stripe(
                cubic: [(-5.87, 62.01), (39.09, 65.44), (48.72, 28.71), (49.03, -6.21)],
                draw: (0, 0.3), erase: (0.55, 0.8)
            ),
            Stripe(
                cubic: [(-5.07, 86), (53.78, 84.42), (71.13, 37.29), (73, -5.09)],
                draw: (0.12, 0.42), erase: (0.65, 0.9)
            ),
            Stripe(
                cubic: [(-4.27, 109.99), (68.46, 103.4), (93.55, 45.86), (96.98, -3.98)],
                draw: (0.24, 0.54), erase: (0.75, 1)
            ),
        ]

        /// The visible part of stripe `index` at loop `phase` (0..1, wrapped),
        /// in the cubic's parameter t: the cut DRAWS IN from t=0 over its
        /// draw window, stays whole, ERASES from t=0 over its erase window,
        /// and is absent outside them — the loop starts and ends on the plain
        /// disc. Rounded to 4 decimals. nil = not drawn.
        public static func stripeSpan(_ index: Int, phase: Double) -> ClosedRange<Double>? {
            guard stripes.indices.contains(index) else { return nil }
            let stripe = stripes[index]
            let p = (phase.truncatingRemainder(dividingBy: 1) + 1).truncatingRemainder(dividingBy: 1)
            if p < stripe.draw.from || p >= stripe.erase.to { return nil }
            if p < stripe.draw.to {
                return 0...round(ease((p - stripe.draw.from) / (stripe.draw.to - stripe.draw.from)))
            }
            if p < stripe.erase.from { return 0...1 }
            return round(ease((p - stripe.erase.from) / (stripe.erase.to - stripe.erase.from)))...1
        }

        /// A point on stripe `index`'s cubic at parameter t, on the 100×100 grid.
        public static func stripePoint(_ index: Int, t: Double) -> (x: Double, y: Double) {
            let p = stripes[index].cubic
            let u = 1 - t
            let a = u * u * u
            let b = 3 * u * u * t
            let c = 3 * u * t * t
            let d = t * t * t
            return (
                a * p[0].x + b * p[1].x + c * p[2].x + d * p[3].x,
                a * p[0].y + b * p[1].y + c * p[2].y + d * p[3].y
            )
        }

        /// Every stripe's span at loop `phase`.
        public static func spans(phase: Double) -> [ClosedRange<Double>?] {
            stripes.indices.map { stripeSpan($0, phase: phase) }
        }

        /// The static logo: all three cuts whole. What the mark draws whenever
        /// it does not animate (Reduce Motion, `animated: false`, a capture, a
        /// paused or slowed timeline) — never the loop's phase 0, which is the
        /// plain disc and reads as a broken mark.
        public static let staticSpans: [ClosedRange<Double>?] = stripes.map { _ in 0...1 }

        /// The middle of the window where every cut is whole (each drawn in,
        /// none erasing yet): the loop at rest, identical to `staticSpans`.
        public static let restPhase: Double = {
            let drawn = stripes.map(\.draw.to).max() ?? 0
            let erasing = stripes.map(\.erase.from).min() ?? 0
            return (drawn + erasing) / 2
        }()

        /// The loop phase at `elapsed` seconds — linear.
        public static func phase(elapsed: TimeInterval) -> Double {
            let loop = Double(loopMs) / 1000
            return (elapsed.truncatingRemainder(dividingBy: loop)) / loop
        }

        /// The phase `elapsed` seconds after the mark's FIRST frame: the loop
        /// is entered at `restPhase`, so that frame is the static logo and the
        /// sweep continues from it without a jump (erase, then draw in again).
        /// A timeline that never advances past its first frame shows the logo.
        public static func phase(sinceStart elapsed: TimeInterval) -> Double {
            phase(elapsed: max(0, elapsed) + restPhase * Double(loopMs) / 1000)
        }

        static func ease(_ x: Double) -> Double {
            x < 0.5 ? 4 * x * x * x : 1 - pow(-2 * x + 2, 3) / 2
        }

        static func round(_ value: Double) -> Double {
            (value * 10_000).rounded() / 10_000
        }
    }

    /// (5) BLOCKING: a full-window backdrop + the mark + ONE muted line —
    /// only switching team, switching account and signing out.
    public enum Blocking {
        public static let backdropOpacity = 0.85
        public static let markSize: Double = 40
        public static let gap: Double = 16

        public static func switchTeam(_ team: String) -> String { "Switching to \(team)" }
        public static func switchAccount(_ account: String) -> String { "Switching to \(account)" }
        public static let signOut = "Signing out"
    }

    /// The composer after Start: the mark + "Starting on <device>" + elapsed,
    /// until the session row lands or `timeoutMs` passes (then a toast).
    public enum Starting {
        public static func label(device: String) -> String { "Starting on \(device)" }
        public static let timeoutMs = 30_000
        /// How long the watch keeps looking for a late row after the send.
        public static let watchMs = 1_800_000
        /// The server and device dedupe a (deviceId, startId) this long.
        public static let dedupeMs = 1_800_000
        public static let timeoutTitle = "The session did not start"
        public static func timeoutDescription(device: String) -> String { "\(device) did not answer." }
        public static let timeoutAction = "Retry"
    }

    /// (6) ERRORS (FEED-79): a failed action is a toast titled
    /// `Couldn't <verb> <thing>`; invalid input stays inline under its field;
    /// automatic or background work (sync, polling, a load on appear or
    /// focus) never toasts.
    public enum Errors {
        public static let retry = "Retry"
        public static let inlineSize: Double = 13

        public static func title(verb: String, thing: String) -> String { "Couldn't \(verb) \(thing)" }

        /// EXP-1391: `errors.mergeRefusal`. A refused merge is the
        /// failed-action toast with Retry; the server also posts it into the
        /// thread. No client reads the tRPC code.
        public enum MergeRefusal {
            public static let handoff = "Sent to the thread."
        }

        /// Where a failure renders, by who started the work (the fixture's
        /// `kind` → `render`): `action` = toast, `input` = inline,
        /// `background` (and anything unknown) = quiet.
        public static func surface(kind: String) -> String {
            switch kind {
            case "action": return "toast"
            case "input": return "inline"
            default: return "quiet"
            }
        }
    }
}
