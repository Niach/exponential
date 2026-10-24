import ExpCore
import ExpUI
import SwiftUI

/// Thread 58cb32df (chats.json `shell`, ×3 phones): the floating bottom
/// bar = the glass TAB CAPSULE (the system Liquid Glass look on iOS 26+, the
/// opaque card composite before it) with the four tabs Board · Devices ·
/// Chats · Team in order, each an icon over its label, the selected one on
/// the travelling glass-active pill; the Team tab's icon is the ACTIVE
/// team's avatar and its label the team's name; a dot on Chats while a chat
/// needs me (amber), runs (green) or holds news (primary). Beside it,
/// unchanged, the SPLIT launcher (chat | new issue): the chat arm opens the
/// agent home (a bar-visible root, it wears the selected circle while up),
/// the New-issue arm files into the board in view, dimmed only while the
/// team has no board at all (EXP-973). The Inbox is no tab any more: it is
/// a page pushed from the Chats header (EXP-1373). Attached via
/// `.overlay(alignment: .bottom)` so content scrolls underneath it; each
/// bar-visible scrollable reserves clearance with `.tabBarBottomInset()`
/// (EXP-36). MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    /// Lit while the agent home (the empty-seed composer page) is up: the
    /// chat arm wears the selected circle and no pill tab is lit.
    let agentActive: Bool
    let boardActive: Bool
    let devicesActive: Bool
    let chatsActive: Bool
    let teamActive: Bool
    /// The active team (the Team tab's avatar and label); nil = none yet.
    let team: TeamEntity?
    /// A chat runs — the Chats dot (green).
    let chatsRunning: Bool
    /// A chat needs me — the dot turns amber.
    let chatsNeedYou: Bool
    /// A chat holds news I have not read — the dot in the primary tone.
    var chatsUnread = false
    /// EXP-973: whether the New-issue arm can go anywhere.
    let composeEnabled: Bool
    let onBoard: () -> Void
    let onDevices: () -> Void
    let onChats: () -> Void
    let onTeam: () -> Void
    let onCompose: () -> Void
    let onChat: () -> Void

    // EXP-523: the active pill SLIDES between tabs: one capsule holds the
    // geometry id at a time, animated from `activeKey` on the row.
    @Namespace private var tabPill
    @Environment(\.motion) private var motion

    private var activeKey: String {
        if boardActive { return "board" }
        if devicesActive { return "devices" }
        if chatsActive { return "chats" }
        if teamActive { return "team" }
        return "none"
    }

    var body: some View {
        HStack(spacing: MobileTabBarMetrics.launcherGap) {
            HStack(spacing: MobileTabBarMetrics.tabSpacing) {
                tab(.board, active: boardActive, action: onBoard)
                    .accessibilityIdentifier("tab-issues")
                tab(.devices, active: devicesActive, action: onDevices)
                    .accessibilityIdentifier("tab-devices")
                tab(
                    .chats,
                    active: chatsActive,
                    badge: chatsRunning || chatsNeedYou || chatsUnread,
                    badgeColor: chatsNeedYou
                        ? DesignTokens.Semantic.yellow
                        : chatsRunning ? DesignTokens.Semantic.green : DesignTokens.Palette.primary,
                    action: onChats
                )
                .accessibilityIdentifier("tab-threads")
                tab(.team, active: teamActive, action: onTeam)
                    .accessibilityIdentifier(Chats.TestIds.tabTeam)
            }
            .animation(motion.standard, value: activeKey)
            .padding(MobileTabBarMetrics.pillPadding)
            .tabBarGlass()

            Spacer(minLength: 0)

            launcherCapsule
        }
        .padding(.horizontal, MobileTabBarMetrics.inset)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    /// EXP-827: the merged launcher — two 52pt arms split by a hairline, as
    /// tall as the tab capsule beside it.
    private var launcherCapsule: some View {
        HStack(spacing: 0) {
            arm(glyph: AppIcons.actionChat, selected: agentActive, action: onChat)
                .accessibilityLabel("Start chat")
                .accessibilityIdentifier("chat-button")
            Rectangle()
                .fill(GlassTokens.strokeStrong)
                .frame(width: GlassTokens.hairline, height: 28)
            arm(
                glyph: AppIcons.navCreateIssue,
                enabled: composeEnabled,
                action: onCompose
            )
            .accessibilityLabel("New issue")
            .accessibilityIdentifier("compose-button")
        }
        .tabBarGlass()
    }

    private func arm(
        glyph: String,
        selected: Bool = false,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            AppIcon(glyph, size: AppIcon.Size.large, weight: .semibold)
                .foregroundStyle(.white.opacity(enabled ? 1 : TextOpacity.tertiary))
                .frame(
                    width: MobileTabBarMetrics.launcherArm,
                    height: MobileTabBarMetrics.barHeight
                )
                .background {
                    if selected {
                        Circle()
                            .fill(GlassTokens.fillActive)
                            .frame(width: 44, height: 44)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }

    private func tab(
        _ which: Chats.Shell.Tab,
        active: Bool,
        badge: Bool = false,
        badgeColor: Color = DesignTokens.Palette.primary,
        action: @escaping () -> Void
    ) -> some View {
        let label = which == .team ? (team?.name ?? which.label) : which.label
        return Button(action: action) {
            VStack(spacing: 3) {
                tabIcon(which, active: active)
                    .frame(width: 26, height: 24)
                    .overlay(alignment: .topTrailing) {
                        if badge {
                            Circle()
                                .fill(badgeColor)
                                .frame(width: 8, height: 8)
                                .offset(x: 3, y: -1)
                        }
                    }
                Text(label)
                    .font(.system(size: MobileTabBarMetrics.labelSize, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .padding(.horizontal, 2)
            }
            .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
            .frame(width: MobileTabBarMetrics.tabWidth, height: MobileTabBarMetrics.tabHeight)
            .background {
                // Only the ACTIVE tab renders the pill, carrying the shared
                // geometry id — that is what makes it travel.
                if active {
                    Capsule()
                        .fill(GlassTokens.fillActive)
                        .matchedGeometryEffect(id: "tab-pill", in: tabPill)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityAddTraits(active ? .isSelected : [])
    }

    @ViewBuilder
    private func tabIcon(_ which: Chats.Shell.Tab, active: Bool) -> some View {
        switch which {
        case .team:
            if let team {
                TeamAvatar(team: team, size: 22)
            } else {
                AppIcon(AppIcons.navSettings, size: AppIcon.Size.large)
            }
        case .board:
            AppIcon(AppIcons.navIssues, size: AppIcon.Size.large)
        case .devices:
            AppIcon(AppIcons.navDevices, size: AppIcon.Size.large)
        case .chats:
            AppIcon(AppIcons.navThreads, size: AppIcon.Size.large)
        }
    }
}

private extension View {
    /// The bar's capsule chrome: the system Liquid Glass on iOS 26+, else
    /// the OPAQUE card composite under the strong hairline (EXP-698: never a
    /// low-alpha tint the feed shows through).
    @ViewBuilder
    func tabBarGlass() -> some View {
        if #available(iOS 26.0, *) {
            self.glassEffect(.regular.tint(GlassTokens.opaqueCardFill.opacity(0.55)).interactive(), in: Capsule())
        } else {
            self
                .background(GlassTokens.opaqueCardFill, in: Capsule())
                .overlay(Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
        }
    }
}

/// The height `.tabBarBottomInset()` reserves — exposed for the one layout
/// that has to subtract it (Home's centred composer).
let tabBarBottomClearance: CGFloat = 88

extension View {
    /// Bottom clearance for the floating MobileTabBar (EXP-36): bar height
    /// (42pt tab frame + 2×5pt pill padding + 8pt top + 4pt bottom = 64pt)
    /// plus 16pt of breathing room. The bar is an ancestor OVERLAY (see
    /// MainNavigator) — ancestor safe-area insets don't reliably reach List
    /// content inside pushed destinations, so every bar-visible scrollable
    /// (Home, Issues list, Devices, Inbox's inbox/my-issues, Threads,
    /// Actions) applies
    /// this ONE modifier directly. Detail screens (showsTabBar == false) must
    /// NOT reserve it — pass `false` when the same scrollable is reused on a
    /// bar-less surface.
    ///
    /// ONE view type whichever way `enabled` goes (EXP-1276): an `if/else`
    /// here gave the modified scroller a NEW identity every time the flag
    /// flipped, and the Agent root flips it with the keyboard — the rebuilt
    /// composer text view re-took first responder, so a dismissed keyboard
    /// came straight back. Off = a zero inset with zero spacing, which lays
    /// out exactly like no inset at all.
    func tabBarBottomInset(_ enabled: Bool = true) -> some View {
        safeAreaInset(edge: .bottom, spacing: enabled ? nil : 0) {
            Color.clear.frame(height: enabled ? tabBarBottomClearance : 0)
        }
    }
}
