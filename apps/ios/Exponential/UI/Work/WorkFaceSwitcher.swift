import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the Work screen's face SWITCHER — the bar's bottom-right circle.
/// One target = a direct switch wearing the destination's glyph; two or more
/// = the faces glyph (✕ while open) over a menu the screen anchors above
/// this circle (`.glassMenuOverlay`, placement flips above a bottom anchor).
/// The badge dot is `WorkFaces.switcherBadge`: the shown session's state off
/// the Run face, green while changes exist on it.
struct WorkFaceSwitcher: View {
    let mode: WorkFaces.SwitcherMode
    let badge: WorkFaces.SwitcherBadge?
    /// EXP-848: the state badge pulses only while the agent is in a turn.
    let badgePulsing: Bool
    @Binding var anchor: CGRect
    @Binding var menuOpen: Bool
    let onSelect: (WorkFaces.SwitcherTarget) -> Void

    var body: some View {
        switch mode {
        case .hidden:
            EmptyView()
        case let .toggle(target):
            FloatingBarCircle(
                accessibilityLabel: Self.label(target),
                action: { onSelect(target) },
                content: { glyph(Self.icon(target)) },
                badge: { badgeDot }
            )
            .accessibilityIdentifier("work-face-switcher")
        case .menu:
            FloatingBarCircle(
                accessibilityLabel: "Switch view",
                action: toggleMenu,
                content: { glyph(menuOpen ? AppIcons.uiClose : AppIcons.workFaces) },
                badge: { badgeDot }
            )
            .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                anchor = frame
            }
            .accessibilityIdentifier("work-face-switcher")
        }
    }

    private func glyph(_ icon: String) -> some View {
        AppIcon(icon, size: AppIcon.Size.medium, weight: .medium)
            .foregroundStyle(.white)
    }

    @ViewBuilder
    private var badgeDot: some View {
        switch badge {
        case let .tone(tone):
            SessionStateDot(tone: tone, pulsing: badgePulsing, size: FloatingBarTokens.badgeSize)
        case .changes:
            FloatingBarBadgeDot(color: DesignTokens.Semantic.green)
        case nil:
            EmptyView()
        }
    }

    private func toggleMenu() {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { menuOpen.toggle() }
    }

    /// A direct switch names its destination: Issue, Run, Changes, Start.
    static func icon(_ target: WorkFaces.SwitcherTarget) -> String {
        switch target {
        case .face(.issue): AppIcons.uiIssue
        case .face(.run), .run: AppIcons.navDevices
        case .face(.changes): AppIcons.codingDiff
        case .startCoding: AppIcons.actionRun
        }
    }

    static func label(_ target: WorkFaces.SwitcherTarget) -> String {
        switch target {
        case let .face(face): WorkFaces.faceLabel(face)
        case .run: WorkFaces.runFaceLabel
        case .startCoding: WorkFaces.startCodingLabel
        }
    }
}
