import ExpCore
import ExpUI
import SwiftUI

/// EXP-1681 (voice-call.json, styleguide entry `call-pill`): THE floating
/// call pill, the desktop's (`voice_overlay::call_pill`) and web's
/// `VoiceCallPill` row: the drag handle, the agent's avatar small, the name
/// over the timer (ringing: the status), VoiceWave sm, then mute and the red
/// end at the compact size. A minimized call draws it
/// ([VoiceCallPillOverlay]); the styleguide specimens draw the same view.
struct VoiceCallPill: View {
    let view: VoiceCall.View
    let callee: VoiceCallCallee
    var timer: VoiceCallSurface.Timer
    var level: Double
    var bands: [Double]
    /// `pill.width`, never wider than the screen minus two margins.
    var width = CGFloat(VoiceCall.Pill.width)
    var animated: Bool = true
    var onMute: () -> Void = {}
    var onEnd: () -> Void = {}

    /// The avatar disc (web `CallAvatar` sm, desktop `AVATAR_SM`).
    static let avatar: CGFloat = 28
    /// The compact round buttons (web `CallButton` sm).
    static let button: CGFloat = 36

    var body: some View {
        HStack(spacing: 8) {
            handle
            avatar
            VStack(alignment: .leading, spacing: 0) {
                Text(callee.name)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(DesignTokens.Palette.foreground)
                status
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
            }
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
            VoiceCallWave(activity: view.activity, level: level, bands: bands, animated: animated, size: VoiceCallWave.sm)
                .opacity(view.activity == .ringing || view.activity == .muted ? 0.35 : 1)
            buttons
        }
        .padding(.leading, 12)
        .padding(.trailing, 10)
        .frame(width: width, height: CGFloat(VoiceCall.Pill.height))
        .background(GlassTokens.fillCard, in: Capsule())
        .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .shadow(color: .black.opacity(0.25), radius: 12, y: 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(VoiceCall.TestIds.pill)
    }

    /// Two columns of three dots: a texture, not a glyph.
    private var handle: some View {
        HStack(spacing: 2) {
            ForEach(0..<2, id: \.self) { _ in
                VStack(spacing: 2) {
                    ForEach(0..<3, id: \.self) { _ in
                        Circle().fill(DesignTokens.Palette.mutedForeground.opacity(0.7)).frame(width: 3, height: 3)
                    }
                }
            }
        }
        .padding(.horizontal, 2)
        .accessibilityHidden(true)
    }

    private var avatar: some View {
        ZStack {
            Circle().fill(DesignTokens.Palette.secondary)
            if let icon = callee.icon {
                AppIcon(icon, size: 16, weight: .medium)
                    .foregroundStyle(Color(hex: callee.color) ?? DesignTokens.Palette.foreground)
            } else {
                ExpLogoMark(size: 16, color: DesignTokens.Palette.foreground)
            }
            if view.activity == .listening {
                Circle().strokeBorder(DesignTokens.Semantic.green.opacity(0.8), lineWidth: 2)
            }
        }
        .frame(width: Self.avatar, height: Self.avatar)
        .opacity(view.activity == .thinking ? 0.6 : 1)
        .accessibilityElement()
        .accessibilityLabel(callee.name)
        .accessibilityValue(VoiceCallSurface.stateWord(view))
    }

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

    /// Mute and end only: the speaker stays on the full screen.
    private var buttons: some View {
        HStack(spacing: 6) {
            ForEach(view.buttons.filter { $0 != .speaker }, id: \.self) { button in
                switch button {
                case .mute:
                    CallButton(
                        icon: view.muted ? AppIcons.callUnmute : AppIcons.callMute,
                        label: view.muted ? VoiceCall.Copy.unmute : VoiceCall.Copy.mute,
                        toggled: view.muted,
                        size: Self.button,
                        action: onMute
                    )
                    .accessibilityIdentifier(VoiceCall.TestIds.mute)
                case .end:
                    CallButton(
                        icon: AppIcons.callEnd, label: view.end, tone: .destructive, size: Self.button, action: onEnd
                    )
                    .accessibilityIdentifier(VoiceCall.TestIds.end)
                case .speaker:
                    EmptyView()
                }
            }
        }
    }
}

/// EXP-1681: a minimized call's pill over every screen (the app root's
/// overlay, under the toasts). Dragged by any point outside its buttons,
/// kept `pill.marginPx` inside the safe area, its centre remembered on the
/// device as a fraction of the safe area; a tap opens the call screen.
struct VoiceCallPillOverlay: View {
    let client: VoiceCallClient

    /// The remembered centre (fractions of the safe area); -1 = the default.
    @AppStorage("voiceCallPill.x") private var savedX: Double = -1
    @AppStorage("voiceCallPill.y") private var savedY: Double = -1
    @State private var drag: CGSize = .zero

    var body: some View {
        GeometryReader { geo in
            let size = geo.size
            let margin = CGFloat(VoiceCall.Pill.marginPx)
            let width = min(CGFloat(VoiceCall.Pill.width), size.width - 2 * margin)
            let height = CGFloat(VoiceCall.Pill.height)
            let rest = clamp(center(in: size, height: height), in: size, width: width, height: height)
            let at = clamp(CGPoint(x: rest.x + drag.width, y: rest.y + drag.height), in: size, width: width, height: height)
            if let view = client.view {
                VoiceCallPill(
                    view: view,
                    callee: client.callee,
                    timer: .since(client.answeredAt),
                    level: client.level,
                    bands: [],
                    width: width,
                    onMute: { client.toggleMute() },
                    onEnd: { client.end() }
                )
                .contentShape(Capsule())
                .onTapGesture { client.expand() }
                .gesture(
                    DragGesture(minimumDistance: 4)
                        .onChanged { drag = $0.translation }
                        .onEnded { value in
                            let end = clamp(
                                CGPoint(x: rest.x + value.translation.width, y: rest.y + value.translation.height),
                                in: size, width: width, height: height
                            )
                            savedX = Double(end.x / max(size.width, 1))
                            savedY = Double(end.y / max(size.height, 1))
                            drag = .zero
                        }
                )
                .position(at)
            }
        }
    }

    /// The remembered centre, else bottom centre `pill.phoneBottomPx` up.
    private func center(in size: CGSize, height: CGFloat) -> CGPoint {
        if savedX >= 0, savedY >= 0 {
            return CGPoint(x: savedX * size.width, y: savedY * size.height)
        }
        return CGPoint(x: size.width / 2, y: size.height - CGFloat(VoiceCall.Pill.phoneBottomPx) - height / 2)
    }

    private func clamp(_ point: CGPoint, in size: CGSize, width: CGFloat, height: CGFloat) -> CGPoint {
        let margin = CGFloat(VoiceCall.Pill.marginPx)
        let x = min(max(point.x, margin + width / 2), max(margin + width / 2, size.width - margin - width / 2))
        let y = min(max(point.y, margin + height / 2), max(margin + height / 2, size.height - margin - height / 2))
        return CGPoint(x: x, y: y)
    }
}
