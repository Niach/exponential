import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2109 (chats.json `shell.dock`, ×3 phones): THE DOCKED BAR: full width
/// at the bottom edge, the opaque `popover` fill over the safe area under one
/// hairline, `shell.tabs` as five equal cells: Chats · Teams · Assistant ·
/// New · Settings, each an icon over its label, muted, the selected one in
/// the foreground colour. THE ASSISTANT cell is the Exponential mark on a
/// round raised disc, its selection a soft glow. NEW never selects: a tap
/// opens THE NEW POPUP above its cell (`shell.newMenu`: Issue, Chat), the
/// cell's icon turned into `openIcon`; a tap outside or on the cell closes it.
///
/// Hung by MainNavigator as an overlay; a page under it reserves
/// `PhoneDockMetrics.clearance` (`phoneDockInset`).
struct PhoneDock: View {
    /// The lit tab (nil = none).
    var selected: Chats.Shell.Tab?
    /// #FEED-232: the Chats tab's count badge, over every team.
    var chatsBadge: NavBadges.Value = .none
    /// The viewer has a board: the popup's Issue row.
    var issueEnabled = true
    /// The New popup is up.
    @Binding var newOpen: Bool
    var onSelect: (Chats.Shell.Tab) -> Void = { _ in }
    var onNew: (Chats.Shell.NewMenu.Item) -> Void = { _ in }
    /// Styleguide only: the bar alone, no full-screen scrim under the popup.
    var specimen = false

    @Environment(\.motion) private var motion

    var body: some View {
        ZStack(alignment: .bottom) {
            if newOpen, !specimen {
                Color.black.opacity(GlassAlertMetrics.scrimOpacity)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { newOpen = false }
                    .accessibilityHidden(true)
                    .transition(.opacity)
            }
            bar
        }
        .animation(motion.standard, value: newOpen)
        // The bar is a fixed `shell.dock` size (its labels are too): a larger
        // text size must not grow the icons over them.
        .dynamicTypeSize(.large)
        // VoiceOver's two-finger scrub closes the popup (the system back).
        .accessibilityAction(.escape) { newOpen = false }
    }

    private var bar: some View {
        HStack(spacing: 0) {
            ForEach(Chats.Shell.tabs, id: \.self) { tab in
                cell(tab)
                    .frame(maxWidth: .infinity)
            }
        }
        .frame(height: PhoneDockMetrics.height)
        .background(alignment: .top) {
            DesignTokens.Palette.popover
                .overlay(alignment: .top) {
                    Rectangle()
                        .fill(GlassTokens.strokeCard)
                        .frame(height: 1)
                }
                .ignoresSafeArea(edges: [.bottom, .horizontal])
        }
    }

    // MARK: - Cells

    @ViewBuilder
    private func cell(_ tab: Chats.Shell.Tab) -> some View {
        let lit = newOpen ? tab == .new : selected == tab
        Button {
            if tab == .new {
                UIApplication.endEditing()
                newOpen.toggle()
            } else {
                newOpen = false
                onSelect(tab)
            }
        } label: {
            if tab == .assistant {
                assistant(selected: lit)
            } else {
                face(tab, lit: lit)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(tab.label)
        .accessibilityAddTraits(lit ? .isSelected : [])
        .accessibilityIdentifier(tab.testId)
        .overlay(alignment: .bottom) {
            if tab == .new, newOpen {
                menu
                    .padding(.bottom, PhoneDockMetrics.height + PhoneDockMetrics.menuGap)
                    .transition(.scale(scale: 0.96, anchor: .bottom).combined(with: .opacity))
            }
        }
        .zIndex(tab == .new ? 1 : 0)
    }

    private func face(_ tab: Chats.Shell.Tab, lit: Bool) -> some View {
        VStack(spacing: PhoneDockMetrics.gap) {
            AppIcon(icon(tab), size: PhoneDockMetrics.icon)
                .frame(width: PhoneDockMetrics.icon, height: PhoneDockMetrics.icon)
                .overlay(alignment: .topTrailing) {
                    if tab == .chats {
                        CountBadge(chatsBadge)
                            .offset(x: 8, y: -6)
                    }
                }
            Text(tab.label)
                .font(.system(size: PhoneDockMetrics.label, weight: .medium))
                .lineLimit(1)
        }
        .foregroundStyle(lit ? DesignTokens.Palette.foreground : DesignTokens.Palette.mutedForeground)
        .frame(maxWidth: .infinity)
        .frame(height: PhoneDockMetrics.height)
        .contentShape(Rectangle())
    }

    private func icon(_ tab: Chats.Shell.Tab) -> String {
        let concept = tab == .new && newOpen ? Chats.Shell.NewMenu.openIcon : tab.icon
        return AppIcons.conceptOrName(concept) ?? AppIcons.uiAdd
    }

    private func assistant(selected: Bool) -> some View {
        Circle()
            .fill(DesignTokens.Palette.card)
            .overlay(Circle().strokeBorder(GlassTokens.strokeCard, lineWidth: 1))
            .overlay { ExpLogoMark(size: PhoneDockMetrics.mark, color: DesignTokens.Palette.foreground) }
            .frame(width: PhoneDockMetrics.disc, height: PhoneDockMetrics.disc)
            .background {
                Circle()
                    .fill(DesignTokens.Palette.background)
                    .padding(-PhoneDockMetrics.ring)
                    .shadow(
                        color: selected ? DesignTokens.Palette.foreground.opacity(0.35) : .clear,
                        radius: PhoneDockMetrics.glow / 2
                    )
            }
            .contentShape(Circle())
            .frame(height: PhoneDockMetrics.height, alignment: .top)
            .offset(y: -PhoneDockMetrics.raise)
    }

    // MARK: - The New popup

    private var menu: some View {
        let shape = RoundedRectangle(cornerRadius: PhoneDockMetrics.menuRadius, style: .continuous)
        return VStack(spacing: 0) {
            ForEach(Chats.Shell.NewMenu.items, id: \.self) { item in
                let enabled = item != .issue || issueEnabled
                Button {
                    newOpen = false
                    onNew(item)
                } label: {
                    HStack(spacing: 14) {
                        AppIcon(AppIcons.conceptOrName(item.icon) ?? AppIcons.uiAdd, size: AppIcon.Size.large)
                            .frame(width: 20, height: 20)
                        Text(item.label)
                            .font(.system(size: 15))
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.white.opacity(enabled ? 1 : TextOpacity.tertiary))
                    .padding(.horizontal, 14)
                    .frame(height: PhoneDockMetrics.menuRowHeight)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(!enabled)
                .accessibilityIdentifier(Chats.TestIds.newMenuRow)
            }
        }
        .padding(6)
        .frame(width: PhoneDockMetrics.menuWidth)
        // THE glass menu fill (GlassMenu's opaque card composite + its tint).
        .background(GlassTokens.opaqueCardFill, in: shape)
        .overlay(shape.fill(Color.white.opacity(GlassMenuTokens.tintOpacity)).allowsHitTesting(false))
        .overlay(shape.stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
        .fixedSize()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Chats.TestIds.newMenu)
    }
}

extension View {
    /// A page under the docked bar: its bottom clearance, as a safe area
    /// INSIDE the page, so every scroller, composer and pinned bar in it
    /// clears the bar. ONE view type whichever way `enabled` goes: an
    /// `if/else` would give the page a new identity (EXP-1276).
    func phoneDockInset(_ enabled: Bool) -> some View {
        safeAreaInset(edge: .bottom, spacing: 0) {
            Color.clear.frame(height: enabled ? PhoneDockMetrics.clearance : 0)
        }
    }

    /// The bulk-selection bar's clearance while it stands in the bar's slot.
    func bulkBarInset(_ enabled: Bool) -> some View {
        safeAreaInset(edge: .bottom, spacing: 0) {
            Color.clear.frame(height: enabled ? FloatingBarTokens.slot + 16 : 0)
        }
    }
}
