import ExpCore
import ExpUI
internal import ExponentialUIPrimitives
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif

/// S-3 (voice-call.json PHONE-CALL UI): the phone's call screen, full screen
/// over the app background. Presented from the root while
/// `VoiceCallClient.view` is non-nil.
struct VoiceCallScreen: View {
    let client: VoiceCallClient
    /// The last view, so the cover's dismissal keeps drawing it.
    @State private var shown: VoiceCall.View?

    var body: some View {
        ZStack {
            AppBackground()
            if let view = client.view ?? shown {
                VoiceCallSurface(
                    view: view,
                    callee: client.callee,
                    timer: .since(client.answeredAt),
                    level: client.level,
                    bands: [],
                    onMute: { client.toggleMute() },
                    onSpeaker: { client.toggleSpeaker() },
                    onEnd: { client.end() }
                )
            }
        }
        .preferredColorScheme(.dark)
        .onAppear { shown = client.view }
        .onChange(of: client.view) { _, view in
            if let view { shown = view }
        }
    }
}

/// Who is called: the agent's name and mark (nil icon = the Exponential
/// logo, the Assistant's mark).
struct VoiceCallCallee: Equatable {
    var name: String
    var icon: String?
    var color: String?
}

/// THE phone-call surface (web `voice-call` entry, the IDE's call panel,
/// Android `VoiceCallSurface`): top to bottom the agent's avatar large with
/// its activity ring, the name, the status line or the timer, the caption
/// (≤ 4 lines), and the round call buttons at the bottom. The live screen
/// and the styleguide specimens draw this one view.
struct VoiceCallSurface: View {
    enum Timer: Equatable {
        /// Ticks from the answer (nil = not answered yet: 00:00).
        case since(Date?)
        /// A specimen's fixed text.
        case fixed(String?)
    }

    let view: VoiceCall.View
    let callee: VoiceCallCallee
    var timer: Timer
    var level: Double
    var bands: [Double]
    /// False for a styleguide specimen: the rest phase, never moving.
    var animated: Bool = true
    var onMute: () -> Void = {}
    var onSpeaker: () -> Void = {}
    var onEnd: () -> Void = {}

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 24).frame(maxHeight: 72)
            VoiceCallAvatar(
                activity: view.activity, callee: callee, level: level, bands: bands, animated: animated
            )
            .accessibilityElement()
            .accessibilityLabel(callee.name)
            .accessibilityValue(Self.stateWord(view))
            Text(callee.name)
                .font(.title2.weight(.semibold))
                .foregroundStyle(DesignTokens.Palette.foreground)
                .lineLimit(1)
                .padding(.top, 20)
            status
                .font(.subheadline)
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .padding(.top, 4)
            Text(view.caption)
                .font(.body)
                .foregroundStyle(DesignTokens.Palette.foreground)
                .multilineTextAlignment(.center)
                .lineLimit(4)
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .frame(minHeight: 0, alignment: .top)
            Spacer(minLength: 24)
            controls
                .padding(.bottom, 40)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(VoiceCall.TestIds.surface)
    }

    /// The status line; nil = the call's timer.
    @ViewBuilder
    private var status: some View {
        if let status = view.status {
            Text(status)
        } else {
            Group {
                switch timer {
                case let .fixed(text):
                    Text(text ?? VoiceCall.callTimer(ms: 0))
                case let .since(start):
                    TimelineView(.periodic(from: start ?? .now, by: 1)) { context in
                        let ms = start.map { Int(context.date.timeIntervalSince($0) * 1000) } ?? 0
                        Text(VoiceCall.callTimer(ms: ms))
                    }
                }
            }
            .monospacedDigit()
            .accessibilityIdentifier(VoiceCall.TestIds.timer)
        }
    }

    private var controls: some View {
        CallControls {
            ForEach(view.buttons, id: \.self) { button in
                switch button {
                case .mute:
                    CallButton(
                        icon: view.muted ? AppIcons.callUnmute : AppIcons.callMute,
                        label: view.muted ? VoiceCall.Copy.unmute : VoiceCall.Copy.mute,
                        toggled: view.muted,
                        action: onMute
                    )
                    .accessibilityIdentifier(VoiceCall.TestIds.mute)
                case .speaker:
                    CallButton(
                        icon: view.speaker ? AppIcons.callSpeaker : AppIcons.callSpeakerOff,
                        label: view.speaker ? VoiceCall.Copy.speaker : VoiceCall.Copy.speakerOff,
                        toggled: !view.speaker,
                        action: onSpeaker
                    )
                    .accessibilityIdentifier(VoiceCall.TestIds.speaker)
                case .end:
                    CallButton(
                        icon: AppIcons.callEnd,
                        label: view.end,
                        tone: .destructive,
                        // Ringing: the label shows under it (Cancel).
                        caption: view.activity == .ringing ? view.end : nil,
                        action: onEnd
                    )
                    .accessibilityIdentifier(VoiceCall.TestIds.end)
                }
            }
        }
    }

    /// The avatar's accessibility value (no state word on screen).
    static func stateWord(_ view: VoiceCall.View) -> String {
        switch view.activity {
        case .ringing: view.status ?? VoiceCall.Copy.calling
        case .listening: VoiceCall.Copy.listening
        case .thinking: VoiceCall.Copy.thinking
        case .speaking: VoiceCall.Copy.speaking
        case .muted: VoiceCall.Copy.muted
        }
    }
}

/// The agent's avatar, large and centred, with a ring that follows the
/// call's activity: ringing = a soft halo pulse, listening = the success
/// ring glowing with the level, speaking = the VoiceWave, thinking = dim,
/// muted = none.
struct VoiceCallAvatar: View {
    let activity: VoiceCall.Activity
    let callee: VoiceCallCallee
    var level: Double
    var bands: [Double]
    var animated: Bool = true

    /// The whole ring area (`VoiceWave` xl), fixed so nothing below moves.
    static let frame: CGFloat = 200
    /// The avatar disc.
    static let disc: CGFloat = 104

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulse = false

    private var moves: Bool { animated && !reduceMotion }

    var body: some View {
        ZStack {
            ring
            avatar
                .opacity(activity == .thinking ? 0.6 : 1)
        }
        .frame(width: Self.frame, height: Self.frame)
        .animation(.easeOut(duration: 0.12), value: level)
    }

    private var avatar: some View {
        ZStack {
            Circle().fill(DesignTokens.Palette.card)
            Circle().strokeBorder(DesignTokens.Palette.border, lineWidth: 1)
            if let icon = callee.icon {
                AppIcon(icon, size: 44, weight: .medium)
                    .foregroundStyle(Color(hex: callee.color) ?? DesignTokens.Palette.foreground)
            } else {
                ExpLogoMark(size: 60, color: DesignTokens.Palette.foreground)
            }
        }
        .frame(width: Self.disc, height: Self.disc)
    }

    @ViewBuilder
    private var ring: some View {
        switch activity {
        case .ringing:
            // Two soft halos breathing out of the disc.
            ZStack {
                Circle()
                    .fill(DesignTokens.Palette.foreground.opacity(0.06))
                    .frame(width: Self.disc + 64, height: Self.disc + 64)
                    .scaleEffect(moves && pulse ? 1.12 : 1)
                    .opacity(moves && pulse ? 0.4 : 1)
                Circle()
                    .fill(DesignTokens.Palette.foreground.opacity(0.08))
                    .frame(width: Self.disc + 28, height: Self.disc + 28)
            }
            .onAppear {
                guard moves else { return }
                withAnimation(.easeInOut(duration: 1.2).repeatForever(autoreverses: true)) { pulse = true }
            }
        case .listening:
            let glow = min(1, max(0, level))
            ZStack {
                Circle()
                    .fill(DesignTokens.Semantic.green.opacity(0.10 + 0.25 * glow))
                    .frame(width: Self.disc + 24 + 40 * glow, height: Self.disc + 24 + 40 * glow)
                    .blur(radius: 10)
                Circle()
                    .strokeBorder(DesignTokens.Semantic.green, lineWidth: 3)
                    .frame(width: Self.disc + 12, height: Self.disc + 12)
            }
        case .speaking:
            VoiceCallWave(activity: .speaking, level: level, bands: bands, animated: animated)
        case .thinking:
            Circle()
                .strokeBorder(DesignTokens.Palette.border, lineWidth: 3)
                .frame(width: Self.disc + 12, height: Self.disc + 12)
        case .muted:
            EmptyView()
        }
    }
}

/// Exponential UI's `VoiceWave` at `xl` (200) around the avatar: the rings
/// come from the core (`VoiceWaveFrame`, `voiceWaveJson`) and
/// `VoiceWaveView` draws them; the avatar sits on top. A build without the
/// core (`canImport(ExponentialUI)` false, no FFI xcframework) draws nothing.
struct VoiceCallWave: View {
    let activity: VoiceCall.Activity
    var level: Double
    var bands: [Double]
    var animated: Bool = true

    /// `VoiceWave` xl (`VOICE_WAVE_SIZE_PX.xl`).
    static let size: CGFloat = 200

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()

    var body: some View {
        content
            .frame(width: Self.size, height: Self.size)
            .accessibilityHidden(true)
    }

    #if canImport(ExponentialUI)
    private var content: some View {
        let animates = animated && !reduceMotion
        return TimelineView(.animation(paused: !animates)) { timeline in
            let elapsed = animates ? timeline.date.timeIntervalSince(start) : 0
            let frame = VoiceWaveFrame.compute(
                size: Self.size, activity: activity.rawValue, level: level, bands: bands,
                elapsed: elapsed, reducedMotion: reduceMotion || !animated
            )
            VoiceWaveView.with(rings: (frame?.rings ?? []).map(ring), size: Self.size, childScale: 0) {
                Color.clear
            }
        }
    }

    private func ring(_ ring: VoiceWaveFrame.Ring) -> VoiceWaveView.Ring {
        VoiceWaveView.Ring(
            points: ring.points, color: ring.part == "outer" ? DesignTokens.Semantic.blue : DesignTokens.Palette.primary,
            fillAlpha: ring.fillAlpha, strokeAlpha: ring.strokeAlpha, strokeWidth: ring.strokeWidth
        )
    }
    #else
    private var content: some View { Color.clear }
    #endif
}
