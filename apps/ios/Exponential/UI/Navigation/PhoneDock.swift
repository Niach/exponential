import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2109 (chats.json `shell.dock`, ×3 phones): THE DOCKED BAR: full width
/// at the bottom edge, the opaque `popover` fill over the safe area under one
/// hairline, `shell.tabs` as five equal cells: Chats · Teams · Assistant ·
/// New · More, each an icon over its label, muted, the selected one in
/// the foreground colour. THE ASSISTANT cell is the Exponential mark on a
/// round raised disc, its selection a soft glow. NEW never selects: a tap
/// opens THE NEW POPUP above its cell (`shell.newMenu`: Issue, Chat), the
/// cell's icon turned into `openIcon`; a tap outside or on the cell closes it.
/// #EXP-2171 MORE never selects either: a tap opens THE same menu upwards
/// from its cell, aligned to its end (`shell.moreMenu`: Devices · CI ·
/// Settings, the rows this app has a page for); the cell wears their badges'
/// sum (nav-badges.json `more`).
///
/// Hung by MainNavigator as an overlay; a page under it reserves
/// `PhoneDockMetrics.clearance` (`phoneDockInset`).
struct PhoneDock: View {
    /// The lit tab (nil = none).
    var selected: Chats.Shell.Tab?
    /// #FEED-232: the Chats tab's count badge, over every team.
    var chatsBadge: NavBadges.Value = .none
    /// nav-badges.json `teams-tab`, `assistant`.
    var teamsBadge: NavBadges.Value = .none
    var assistantBadge: NavBadges.Value = .none
    /// The viewer has a board: the popup's Issue row.
    var issueEnabled = true
    /// The New popup is up.
    @Binding var newOpen: Bool
    /// #EXP-2171: the More menu is up.
    @Binding var moreOpen: Bool
    /// The More menu's rows (`MoreMenu.items(available:)`), each with its badge.
    var moreItems: [Chats.Shell.MoreMenu.Item] = []
    var moreBadges: [Chats.Shell.MoreMenu.Item: NavBadges.Value] = [:]
    var onSelect: (Chats.Shell.Tab) -> Void = { _ in }
    var onNew: (Chats.Shell.NewMenu.Item) -> Void = { _ in }
    var onMore: (Chats.Shell.MoreMenu.Item) -> Void = { _ in }
    /// Styleguide only: the bar alone, no full-screen scrim under the popup.
    var specimen = false

    @Environment(\.motion) private var motion

    var body: some View {
        ZStack(alignment: .bottom) {
            if newOpen || moreOpen, !specimen {
                Color.black.opacity(GlassAlertMetrics.scrimOpacity)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { close() }
                    .accessibilityHidden(true)
                    .transition(.opacity)
            }
            bar
        }
        .animation(motion.standard, value: newOpen)
        .animation(motion.standard, value: moreOpen)
        // The bar is a fixed `shell.dock` size (its labels are too): a larger
        // text size must not grow the icons over them.
        .dynamicTypeSize(.large)
        // VoiceOver's two-finger scrub closes the popup (the system back).
        .accessibilityAction(.escape) { close() }
    }

    private func close() {
        newOpen = false
        moreOpen = false
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
        let lit = newOpen ? tab == .new : moreOpen ? tab == .more : selected == tab
        Button {
            if tab == .new {
                UIApplication.endEditing()
                moreOpen = false
                newOpen.toggle()
            } else if tab == .more {
                UIApplication.endEditing()
                newOpen = false
                moreOpen.toggle()
            } else {
                close()
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
        .accessibilityValue(NavBadges.countBadgeLabel(badge(tab).count))
        .accessibilityAddTraits(lit ? .isSelected : [])
        .accessibilityIdentifier(tab.testId)
        .overlay(alignment: .bottom) {
            if tab == .new, newOpen {
                menu
                    .padding(.bottom, PhoneDockMetrics.height + PhoneDockMetrics.menuGap)
                    .transition(.scale(scale: 0.96, anchor: .bottom).combined(with: .opacity))
            }
        }
        // align end: the menu's trailing edge on the cell's.
        .overlay(alignment: .bottomTrailing) {
            if tab == .more, moreOpen {
                more
                    .padding(.trailing, 8)
                    .padding(.bottom, PhoneDockMetrics.height + PhoneDockMetrics.menuGap)
                    .transition(.scale(scale: 0.96, anchor: .bottomTrailing).combined(with: .opacity))
            }
        }
        .zIndex(tab == .new || tab == .more ? 1 : 0)
    }

    private func face(_ tab: Chats.Shell.Tab, lit: Bool) -> some View {
        VStack(spacing: PhoneDockMetrics.gap) {
            AppIcon(icon(tab), size: PhoneDockMetrics.icon)
                .frame(width: PhoneDockMetrics.icon, height: PhoneDockMetrics.icon)
                .overlay(alignment: .topTrailing) {
                    CountBadge(badge(tab))
                        .offset(x: 8, y: -6)
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

    private func badge(_ tab: Chats.Shell.Tab) -> NavBadges.Value {
        switch tab {
        case .chats: chatsBadge
        case .teams: teamsBadge
        case .assistant: assistantBadge
        case .more: NavBadges.sumBadges(moreItems.map { moreBadges[$0] ?? .none })
        default: .none
        }
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
            .overlay(alignment: .topTrailing) {
                CountBadge(assistantBadge)
                    .offset(x: 2, y: -2)
            }
            .contentShape(Circle())
            .frame(height: PhoneDockMetrics.height, alignment: .top)
            .offset(y: -PhoneDockMetrics.raise)
    }

    // MARK: - The New popup

    private var menu: some View {
        popup {
            ForEach(Chats.Shell.NewMenu.items, id: \.self) { item in
                popupRow(icon: item.icon, label: item.label, enabled: item != .issue || issueEnabled) {
                    newOpen = false
                    onNew(item)
                }
                .accessibilityIdentifier(Chats.TestIds.newMenuRow)
            }
        }
        .accessibilityIdentifier(Chats.TestIds.newMenu)
    }

    /// #EXP-2171 `shell.moreMenu`: each row its page, its count badge.
    private var more: some View {
        popup {
            ForEach(moreItems, id: \.self) { item in
                popupRow(icon: item.icon, label: item.label, badge: moreBadges[item] ?? .none) {
                    moreOpen = false
                    onMore(item)
                }
                .accessibilityIdentifier("more-menu-\(item.rawValue)")
            }
        }
        .accessibilityIdentifier("more-menu")
    }

    private func popupRow(
        icon: String, label: String, enabled: Bool = true, badge: NavBadges.Value = .none, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 14) {
                AppIcon(AppIcons.conceptOrName(icon) ?? AppIcons.uiAdd, size: AppIcon.Size.large)
                    .frame(width: 20, height: 20)
                Text(label)
                    .font(.system(size: 15))
                Spacer(minLength: 0)
                CountBadge(badge)
            }
            .foregroundStyle(.white.opacity(enabled ? 1 : TextOpacity.tertiary))
            .padding(.horizontal, 14)
            .frame(height: PhoneDockMetrics.menuRowHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }

    private func popup<Rows: View>(@ViewBuilder _ rows: () -> Rows) -> some View {
        let shape = RoundedRectangle(cornerRadius: PhoneDockMetrics.menuRadius, style: .continuous)
        return VStack(spacing: 0) { rows() }
        .padding(6)
        .frame(width: PhoneDockMetrics.menuWidth)
        // THE glass menu fill (GlassMenu's opaque card composite + its tint).
        .background(GlassTokens.opaqueCardFill, in: shape)
        .overlay(shape.fill(Color.white.opacity(GlassMenuTokens.tintOpacity)).allowsHitTesting(false))
        .overlay(shape.stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
        .fixedSize()
        .accessibilityElement(children: .contain)
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
