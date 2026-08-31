import ExpUI
import SwiftUI

/// Linear-style floating bottom navigation: a glass pill with the top-level
/// destinations (Issues, My Work — with an unread dot — Support — the team
/// helpdesk inbox, present only while the active team's helpdesk flag is on
/// (EXP-180) — Devices — with a running-session dot — Actions — the team's
/// actions/automations surface, its own entry per EXP-686 — and Reviews — its
/// own entry per EXP-147; base order per EXP-81) plus a detached circular
/// button on the right — compose an issue on the board surfaces, start a chat
/// on Devices and Actions (EXP-631/EXP-694). Search is no longer a tab (EXP-686): it is a pushed
/// detail reached from the board header. Attached via
/// `.overlay(alignment: .bottom)` so content
/// scrolls underneath it; each bar-visible scrollable reserves clearance with
/// `.tabBarBottomInset()` (EXP-36). MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    let issuesActive: Bool
    let devicesActive: Bool
    let actionsActive: Bool
    let myWorkActive: Bool
    let reviewsActive: Bool
    let supportActive: Bool
    let unreadCount: Int
    let agentsRunning: Bool
    let agentsNeedInput: Bool
    let reviewsOpen: Bool
    let showsSupport: Bool
    let supportUnread: Bool
    let showsCompose: Bool
    /// EXP-631: the Devices surface puts a Chat launcher in the compose slot —
    /// composing an issue is board-scoped and hidden there anyway. EXP-694:
    /// the Actions surface does the same.
    let showsChat: Bool
    let onIssues: () -> Void
    let onDevices: () -> Void
    let onActions: () -> Void
    let onMyWork: () -> Void
    let onReviews: () -> Void
    let onSupport: () -> Void
    let onCompose: () -> Void
    let onChat: () -> Void

    // EXP-523: the active pill SLIDES between tabs instead of cutting. One
    // capsule holds the geometry id at a time (the standard matched-geometry
    // pattern), and the animation is driven from `activeKey` on the row —
    // the per-tab `active` booleans change together, so animating each tab
    // independently would cross-fade two pills instead of moving one.
    @Namespace private var tabPill
    @Environment(\.motion) private var motion

    /// Which tab currently owns the pill. `none` is reachable (a pushed
    /// detail surface can leave every tab inactive), and simply leaves the
    /// pill unmounted.
    private var activeKey: String {
        if issuesActive { return "issues" }
        if myWorkActive { return "mywork" }
        if supportActive { return "support" }
        if devicesActive { return "devices" }
        if actionsActive { return "actions" }
        if reviewsActive { return "reviews" }
        return "none"
    }

    var body: some View {
        HStack(spacing: 12) {
            // Six tabs (helpdesk on) must still fit a 375pt screen (SE/mini)
            // beside the compose circle: drop the inter-tab spacing and pull
            // the outer padding in — the 44pt touch targets stay intact.
            HStack(spacing: showsSupport ? 0 : 4) {
                tab(glyph: AppIcons.navIssues, label: "Issues", active: issuesActive, action: onIssues)
                    .accessibilityIdentifier("tab-issues")
                // EXP-58: the Inbox tab became My Work (Inbox + My Issues
                // merged) — same glyph, same unread dot.
                tab(
                    glyph: AppIcons.navInbox,
                    label: "My Work",
                    active: myWorkActive,
                    badge: unreadCount > 0,
                    badgeColor: DesignTokens.Palette.primary,
                    action: onMyWork
                )
                .accessibilityIdentifier("tab-mywork")
                // Support (EXP-180): the team helpdesk inbox — the web
                // sidebar's LifeBuoy entry. Present only while the active
                // team's synced helpdesk flag is on.
                if showsSupport {
                    tab(
                        glyph: AppIcons.navSupport,
                        label: "Support",
                        active: supportActive,
                        badge: supportUnread,
                        badgeColor: DesignTokens.Palette.primary,
                        action: onSupport
                    )
                    .accessibilityIdentifier("tab-support")
                }
                // Devices (EXP-686, the renamed Agents surface): the machine
                // list plus its sessions.
                tab(
                    glyph: AppIcons.navDevices,
                    label: "Devices",
                    active: devicesActive,
                    badge: agentsRunning,
                    // Amber while any session waits on a plan approval /
                    // question (EXP-214), live green otherwise.
                    badgeColor: agentsNeedInput
                        ? DesignTokens.Semantic.yellow
                        : DesignTokens.Semantic.green,
                    action: onDevices
                )
                .accessibilityIdentifier("tab-devices")
                // Actions (EXP-686): actions / automations / suggestions, no
                // longer a push off the Devices toolbar.
                tab(
                    glyph: AppIcons.navActions,
                    label: "Actions",
                    active: actionsActive,
                    action: onActions
                )
                .accessibilityIdentifier("tab-actions")
                // Reviews sits last (EXP-147/EXP-152/EXP-686) — the same
                // open-PR glyph the in_review status uses. Green dot while
                // open PRs await review (EXP-214).
                tab(
                    glyph: AppIcons.navReviews,
                    label: "Reviews",
                    active: reviewsActive,
                    badge: reviewsOpen,
                    badgeColor: DesignTokens.Semantic.green,
                    action: onReviews
                )
                .accessibilityIdentifier("tab-reviews")
            }
            .animation(motion.standard, value: activeKey)
            .padding(5)
            .background(.ultraThinMaterial, in: Capsule())
            .overlay(
                Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)

            Spacer()

            if showsCompose {
                fab(glyph: AppIcons.navCreateIssue, action: onCompose)
                    .accessibilityLabel("New issue")
                    .accessibilityIdentifier("compose-button")
            } else if showsChat {
                fab(glyph: AppIcons.actionChat, action: onChat)
                    .accessibilityLabel("Start chat")
                    .accessibilityIdentifier("chat-button")
            }
        }
        .padding(.horizontal, showsSupport ? 12 : 20)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    /// The detached circular button beside the pill — one slot, whatever the
    /// active surface puts in it (compose an issue, start a chat).
    private func fab(glyph: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            AppIcon(glyph, size: AppIcon.Size.large, weight: .semibold)
                .foregroundStyle(.white)
                .frame(width: 52, height: 52)
                .background(.ultraThinMaterial, in: Circle())
                .overlay(
                    Circle().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                )
                .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
        }
        .buttonStyle(.plain)
    }

    private func tab(
        glyph: String,
        label: String,
        active: Bool,
        badge: Bool = false,
        badgeColor: Color = DesignTokens.Palette.primary,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            AppIcon(glyph, size: AppIcon.Size.large)
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                // 44pt (HIG minimum) instead of the old 56pt: up to six tabs
                // (Support present) + the compose circle must fit a 375pt
                // screen (SE/mini) — see the spacing/padding trims in `body`.
                .frame(width: 44, height: 42)
                .overlay(alignment: .topTrailing) {
                    if badge {
                        Circle()
                            .fill(badgeColor)
                            .frame(width: 8, height: 8)
                            .offset(x: -8, y: 8)
                    }
                }
                .background {
                    // Only the ACTIVE tab renders the capsule, and it carries
                    // the shared geometry id — that is what makes it travel.
                    if active {
                        Capsule()
                            .fill(Color.white.opacity(0.12))
                            .matchedGeometryEffect(id: "tab-pill", in: tabPill)
                    }
                }
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

extension View {
    /// Bottom clearance for the floating MobileTabBar (EXP-36): bar height
    /// (42pt tab frame + 2×5pt pill padding + 8pt top + 4pt bottom = 64pt)
    /// plus 16pt of breathing room. The bar is an ancestor OVERLAY (see
    /// MainNavigator) — ancestor safe-area insets don't reliably reach List
    /// content inside pushed destinations, so every bar-visible scrollable
    /// (Issues list, Devices, Actions, My Work's inbox/my-issues,
    /// Reviews) applies
    /// this ONE modifier directly. Detail screens (showsTabBar == false) must
    /// NOT reserve it — pass `false` when the same scrollable is reused on a
    /// bar-less surface.
    @ViewBuilder
    func tabBarBottomInset(_ enabled: Bool = true) -> some View {
        if enabled {
            safeAreaInset(edge: .bottom) {
                Color.clear.frame(height: 80)
            }
        } else {
            self
        }
    }
}
