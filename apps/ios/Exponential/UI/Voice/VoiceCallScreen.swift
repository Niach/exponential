import ExpCore
import ExpUI
internal import ExponentialUIPrimitives
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif

/// NEW-69 (voice-call.json): the phone's call surface, full screen over the
/// app background: THE card of the styleguide entry `voice-call` centred.
/// Presented from the root while `VoiceCallClient.view` is non-nil.
struct VoiceCallScreen: View {
    let client: VoiceCallClient
    /// The last view, so the cover's dismissal keeps drawing it.
    @State private var shown: VoiceCall.View?

    var body: some View {
        ZStack {
            AppBackground()
            if let view = client.view ?? shown {
                VoiceCallCard(
                    view: view,
                    level: client.level,
                    bands: [],
                    onEnd: { client.end() },
                    onMute: { client.toggleMute() }
                )
                .padding(.horizontal, 16)
            }
        }
        .preferredColorScheme(.dark)
        .onAppear { shown = client.view }
        .onChange(of: client.view) { _, view in
            if let view { shown = view }
        }
    }
}

/// THE call card (web `apps/styleguide/src/entries/voice-call.tsx`, the IDE
/// twin `styleguide/entries/voice_call.rs`): the glass card holding VoiceWave
/// xl with the brand mark, the state, the line, then End call (the one
/// primary pill) and Mute. The live screen and the styleguide specimens draw
/// this one view.
struct VoiceCallCard: View {
    let view: VoiceCall.View
    var level: Double
    var bands: [Double]
    /// False for a styleguide specimen: the rest phase, never moving.
    var animated: Bool = true
    var onEnd: () -> Void = {}
    var onMute: () -> Void = {}

    /// The web card's 22rem.
    static let width: CGFloat = 352

    var body: some View {
        VStack(spacing: 16) {
            VoiceCallWave(activity: view.activity, level: level, bands: bands, animated: animated)
            VStack(spacing: 4) {
                Text(view.state)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(DesignTokens.Palette.foreground)
                // An empty line keeps its height (the web's empty span).
                Text(view.line.isEmpty ? " " : view.line)
                    .font(.subheadline)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: 8) {
                GlassPill(VoiceCall.Copy.endCall, size: .md, mode: .action(onEnd), primary: true)
                    .accessibilityIdentifier(VoiceCall.TestIds.end)
                GlassPill(view.mute, size: .md, mode: .action(onMute))
                    .accessibilityIdentifier(VoiceCall.TestIds.mute)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(24)
        .frame(maxWidth: Self.width)
        .glassCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(VoiceCall.TestIds.surface)
    }
}

/// Exponential UI's `VoiceWave` at `xl` (200) with the brand mark (56) as its
/// child: the rings come from the core (`VoiceWaveFrame`, `voiceWaveJson`)
/// and `VoiceWaveView` draws them. A build without the core
/// (`canImport(ExponentialUI)` false, no FFI xcframework) draws the mark
/// alone, like the plan card's surface placeholder.
struct VoiceCallWave: View {
    let activity: VoiceCall.Activity
    var level: Double
    var bands: [Double]
    var animated: Bool = true

    /// `VoiceWave` xl (`VOICE_WAVE_SIZE_PX.xl`).
    static let size: CGFloat = 200
    /// The brand mark (the web island's 56).
    static let mark: CGFloat = 56

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()

    /// Muted paints quiet: the muted ink, the wave at .35.
    private var quiet: Bool { activity == .muted }

    var body: some View {
        content
            .opacity(quiet ? 0.35 : 1)
            .frame(width: Self.size, height: Self.size)
            .accessibilityElement()
            .accessibilityLabel("Voice, \(activity.rawValue)")
    }

    private var mark: some View {
        ExpLogoMark(
            size: Self.mark,
            color: quiet ? DesignTokens.Palette.mutedForeground : DesignTokens.Palette.foreground
        )
    }

    #if canImport(ExponentialUI)
    private static let childScale = VoiceWaveFrame.childBox(size: size, child: CGSize(width: mark, height: mark))?.scale ?? 0

    private var content: some View {
        let animates = animated && !reduceMotion && activity != .muted
        return TimelineView(.animation(paused: !animates)) { timeline in
            let elapsed = animates ? timeline.date.timeIntervalSince(start) : 0
            let frame = VoiceWaveFrame.compute(
                size: Self.size, activity: activity.rawValue, level: level, bands: bands,
                elapsed: elapsed, reducedMotion: reduceMotion || !animated
            )
            VoiceWaveView.with(rings: (frame?.rings ?? []).map(ring), size: Self.size, childScale: Self.childScale) {
                mark
            }
        }
    }

    private func ring(_ ring: VoiceWaveFrame.Ring) -> VoiceWaveView.Ring {
        VoiceWaveView.Ring(
            points: ring.points, color: color(ring.part),
            fillAlpha: ring.fillAlpha, strokeAlpha: ring.strokeAlpha, strokeWidth: ring.strokeWidth
        )
    }

    /// The recipe's ring inks: outer blue, mid + inner primary; quiet = muted.
    private func color(_ part: String) -> Color {
        if quiet { return DesignTokens.Palette.mutedForeground }
        return part == "outer" ? DesignTokens.Semantic.blue : DesignTokens.Palette.primary
    }
    #else
    /// The child rule's scale for the mark without the core: xl's
    /// childSize (style.json `childScaleLarge` × size) over the mark's edge.
    private static let childScale = min(1, 0.22 * size / mark)

    private var content: some View {
        VoiceWaveView.with(rings: [], size: Self.size, childScale: Self.childScale) { mark }
    }
    #endif
}
