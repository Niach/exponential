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
    /// EXP-931: where the switcher is standing. `circle` is the work bar's
    /// glass slot; `inline` is the EXPANDED composer's control row, where it
    /// sits beside the usage ring and wears exactly a composer tool's chrome —
    /// the bar (and its circle) are covered while the composer is open, and
    /// the linked Issue / Changes / Results must stay reachable.
    enum Variant {
        case circle
        case inline
    }

    let mode: WorkFaces.SwitcherMode
    let badge: WorkFaces.SwitcherBadge?
    /// EXP-848: the state badge pulses only while the agent is in a turn.
    let badgePulsing: Bool
    @Binding var anchor: CGRect
    @Binding var menuOpen: Bool
    let onSelect: (WorkFaces.SwitcherTarget) -> Void

    /// EXP-931: set by the composer around its own copy; exactly ONE of the
    /// two is mounted at a time, so both drive the same anchor and the same
    /// menu.
    @Environment(\.workFaceSwitcherVariant) private var variant

    var body: some View {
        switch mode {
        case .hidden:
            EmptyView()
        case let .toggle(target):
            button(
                accessibilityLabel: Self.label(target),
                icon: Self.icon(target),
                action: { onSelect(target) }
            )
        case .menu:
            button(
                accessibilityLabel: "Switch view",
                icon: menuOpen ? AppIcons.uiClose : AppIcons.workFaces,
                action: toggleMenu
            )
        }
    }

    @ViewBuilder
    private func button(
        accessibilityLabel: String, icon: String, action: @escaping () -> Void
    ) -> some View {
        Group {
            switch variant {
            case .circle:
                FloatingBarCircle(
                    accessibilityLabel: accessibilityLabel,
                    action: action,
                    content: { glyph(icon) },
                    badge: { badgeDot }
                )
            case .inline:
                // The ring's neighbour: a composer TOOL, drawn exactly like
                // the `+` and the usage ring beside it.
                Button(action: action) {
                    AppIcon(icon, size: GlassComposerTokens.toolGlyphSize, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(
                            width: GlassComposerTokens.toolHitSize,
                            height: GlassComposerTokens.toolHitSize
                        )
                        .overlay(alignment: .topTrailing) {
                            badgeDot.padding(6)
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(accessibilityLabel)
            }
        }
        // The menu hangs off whichever of the two is standing.
        .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
            anchor = frame
        }
        .accessibilityIdentifier("work-face-switcher")
    }

    private func glyph(_ icon: String) -> some View {
        // EXP-916: Android's switcher glyph is 22dp — the one circle whose
        // mark sits a step above the bar's 20pt.
        AppIcon(icon, size: 22, weight: .medium)
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

    /// A direct switch names its destination: Issue, Run, Changes, Results,
    /// Start.
    static func icon(_ target: WorkFaces.SwitcherTarget) -> String {
        switch target {
        case .face(.issue): AppIcons.uiIssue
        case .face(.run), .run: AppIcons.navDevices
        case .face(.changes): AppIcons.codingDiff
        // EXP-879: the run's published screenshots.
        case .face(.results): AppIcons.workResults
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

/// EXP-931: the switcher's shape travels down the tree, not through every
/// caller — the screen hands its ONE switcher view to four faces, and only the
/// expanded composer wants it composer-sized.
private struct WorkFaceSwitcherVariantKey: EnvironmentKey {
    static let defaultValue = WorkFaceSwitcher.Variant.circle
}

extension EnvironmentValues {
    var workFaceSwitcherVariant: WorkFaceSwitcher.Variant {
        get { self[WorkFaceSwitcherVariantKey.self] }
        set { self[WorkFaceSwitcherVariantKey.self] = newValue }
    }
}
