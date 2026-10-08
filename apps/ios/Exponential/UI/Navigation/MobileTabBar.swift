import ExpUI
import SwiftUI

/// Linear-style floating bottom navigation: a glass pill with the top-level
/// destinations (Issues, Inbox — with an unread dot — Devices — the
/// machines surface — Reviews — its own entry per EXP-147 — and Actions —
/// EXP-1187: a top-level tab ×3, the More menu is gone; Settings stays the
/// Issues header's gear; base order per EXP-81) plus a detached launcher on
/// the right: the SPLIT capsule (chat | new issue), on every bar-visible
/// route since EXP-973 — the chat arm opens the Agent page (the sessions list
/// lives there since EXP-825, so it wears the running-session dot the Devices
/// tab used to carry) and the New-issue arm files into the board in view,
/// dimmed only while the team has no board at all. The Agent page is a
/// bar-visible ROOT and the app's landing screen (mobile parity ×3): the chat
/// arm switches to it rather than pushing, and wears the tabs' selected
/// circle while it is up; only New issue still pushes on top of the tabs. Search is no longer a tab
/// (EXP-686): it is a pushed detail reached from the board header. Attached via
/// `.overlay(alignment: .bottom)` so content
/// scrolls underneath it; each bar-visible scrollable reserves clearance with
/// `.tabBarBottomInset()` (EXP-36). MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    /// Lit while the Agent tab ROOT (the empty-seed Agent page) is up: the
    /// chat arm wears the selected circle and no pill tab is lit.
    let agentActive: Bool
    let issuesActive: Bool
    let devicesActive: Bool
    /// EXP-1187: lit while the Actions list (or an action) is up.
    let actionsActive: Bool
    let myWorkActive: Bool
    let reviewsActive: Bool
    let unreadCount: Int
    let agentsRunning: Bool
    let agentsNeedInput: Bool
    /// EXP-1244: `ReviewsQueue.nav(...).dot` — the Reviews queue is non-empty.
    let reviewsOpen: Bool
    /// EXP-1244: `ReviewsQueue.nav(...).shows` — false only while every team
    /// runs in yolo mode (EXP-1105: PRs auto-merge) and nothing is queued.
    var showsReviews: Bool = true
    /// EXP-973: whether the New-issue arm can go anywhere. The split capsule
    /// itself rides EVERY bar-visible route now; only a team with no board at
    /// all leaves the arm dimmed and inert.
    let composeEnabled: Bool
    let onIssues: () -> Void
    let onDevices: () -> Void
    let onActions: () -> Void
    let onMyWork: () -> Void
    let onReviews: () -> Void
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
        if devicesActive { return "devices" }
        if reviewsActive { return "reviews" }
        if actionsActive { return "actions" }
        return "none"
    }

    var body: some View {
        HStack(spacing: MobileTabBarMetrics.launcherGap) {
            // EXP-973: five tabs must fit a 375pt screen (SE/mini) beside the
            // 104pt SPLIT launcher, which rides every route —
            // `MobileTabBarMetrics` holds the budget and its test does the
            // arithmetic.
            HStack(spacing: MobileTabBarMetrics.tabSpacing) {
                tab(glyph: AppIcons.navIssues, label: "Issues", active: issuesActive, action: onIssues)
                    .accessibilityIdentifier("tab-issues")
                // EXP-58: Inbox + My Issues merged behind this one tab (SLOP-5:
                // called Inbox, like the web and desktop entry) — same glyph,
                // same unread dot.
                tab(
                    glyph: AppIcons.navInbox,
                    label: "Inbox",
                    active: myWorkActive,
                    badge: unreadCount > 0,
                    badgeColor: DesignTokens.Palette.primary,
                    action: onMyWork
                )
                .accessibilityIdentifier("tab-mywork")
                // Devices (EXP-686, the renamed Agents surface): the machine
                // list. Its live dot moved to the Agent launcher with the
                // sessions list.
                tab(
                    glyph: AppIcons.navDevices,
                    label: "Devices",
                    active: devicesActive,
                    action: onDevices
                )
                .accessibilityIdentifier("tab-devices")
                // Reviews (EXP-147/EXP-152/EXP-686) — the same
                // open-PR glyph the in_review status uses. Green dot while
                // open PRs await review (EXP-214).
                // EXP-1105: hidden in yolo mode unless a PR is open.
                if showsReviews {
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
                // EXP-1187: Actions — a top-level tab ×3 (the More menu is
                // gone; Settings is the Issues header's gear). Authoring
                // lives here; the composer's action chip runs one.
                tab(
                    glyph: AppIcons.navActions,
                    label: "Actions",
                    active: actionsActive,
                    action: onActions
                )
                .accessibilityIdentifier("tab-actions")
            }
            .animation(motion.standard, value: activeKey)
            .padding(MobileTabBarMetrics.pillPadding)
            // EXP-698: flat, not blurred — the pill floats over scrolling
            // content, so its fill is the OPAQUE composite (`fillCard` over
            // the card surface), never a low-alpha tint the feed shows through.
            .background(GlassTokens.opaqueCardFill, in: Capsule())
            .overlay(
                Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
            )

            Spacer()

            // The Chat launcher opens the Agent page from every top-level
            // surface (EXP-631/694/827, and the rest since the sessions list
            // moved there): it wears the live dot — amber while any of my
            // sessions waits on a plan approval / question (EXP-214), green
            // while one runs. EXP-973: New issue sits beside it on EVERY
            // route, not only a board — the capsule is the bar's ONE launcher.
            launcherCapsule
        }
        .padding(.horizontal, MobileTabBarMetrics.inset)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    /// EXP-827: the merged launcher on a board — the same 52pt height as the
    /// circle, two 52pt arms split by a hairline, each its own button with
    /// the labels and identifiers the single circles carry.
    /// The launcher's dot color: nil while nothing of mine runs.
    private var agentBadge: Color? {
        guard agentsRunning else { return nil }
        return agentsNeedInput ? DesignTokens.Semantic.yellow : DesignTokens.Semantic.green
    }

    private var launcherCapsule: some View {
        HStack(spacing: 0) {
            arm(glyph: AppIcons.actionChat, badge: agentBadge, selected: agentActive, action: onChat)
                .accessibilityLabel("Start chat")
                .accessibilityIdentifier("chat-button")
            Rectangle()
                .fill(GlassTokens.strokeStrong)
                .frame(width: GlassTokens.hairline, height: 28)
            // EXP-973: with no board in the team there is nowhere to file an
            // issue — the arm stays in place, dimmed and inert, rather than
            // the capsule changing shape from route to route.
            arm(
                glyph: AppIcons.navCreateIssue,
                enabled: composeEnabled,
                action: onCompose
            )
            .accessibilityLabel("New issue")
            .accessibilityIdentifier("compose-button")
        }
        .background(GlassTokens.opaqueCardFill, in: Capsule())
        .overlay(
            Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
        )
    }

    /// One arm of the launcher capsule: a 52pt square hit area, no chrome of
    /// its own (the capsule paints it).
    private func arm(
        glyph: String,
        badge: Color? = nil,
        selected: Bool = false,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            AppIcon(glyph, size: AppIcon.Size.large, weight: .semibold)
                .foregroundStyle(.white.opacity(enabled ? 1 : TextOpacity.tertiary))
                .frame(
                    width: MobileTabBarMetrics.launcherArm,
                    height: MobileTabBarMetrics.launcherArm
                )
                .background {
                    // The tabs' selected circle (the ONE bright glass fill),
                    // a tab-sized 44pt disc centred in the 52pt arm.
                    if selected {
                        Circle()
                            .fill(GlassTokens.fillActive)
                            .frame(
                                width: MobileTabBarMetrics.tabWidth,
                                height: MobileTabBarMetrics.tabHeight
                            )
                    }
                }
                .overlay(alignment: .topTrailing) { launcherDot(badge) }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }

    /// The launcher's status dot — the tab's 8pt disc at the same spot
    /// relative to the glyph (its top-trailing corner), re-based on the
    /// 52pt square.
    @ViewBuilder
    private func launcherDot(_ color: Color?) -> some View {
        if let color {
            Circle()
                .fill(color)
                .frame(width: 8, height: 8)
                .offset(x: -12, y: 12)
        }
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
            tabLabel(glyph: glyph, active: active, badge: badge, badgeColor: badgeColor)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    /// One tab's ink — the glyph in its slot, the travelling active circle and
    /// the status dot — without the button.
    private func tabLabel(
        glyph: String,
        active: Bool,
        badge: Bool = false,
        badgeColor: Color = DesignTokens.Palette.primary
    ) -> some View {
            AppIcon(glyph, size: AppIcon.Size.large)
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                // 44pt square (the HIG minimum) — `MobileTabBarMetrics`
                // owns the budget.
                .frame(
                    width: MobileTabBarMetrics.tabWidth,
                    height: MobileTabBarMetrics.tabHeight
                )
                .overlay(alignment: .topTrailing) {
                    if badge {
                        Circle()
                            .fill(badgeColor)
                            .frame(width: 8, height: 8)
                            .offset(x: -8, y: 8)
                    }
                }
                .background {
                    // Only the ACTIVE tab renders the shape, and it carries
                    // the shared geometry id — that is what makes it travel.
                    // EXP-698: a CIRCLE in the ONE bright glass fill, not a
                    // hand-typed white .12 capsule.
                    if active {
                        Circle()
                            .fill(GlassTokens.fillActive)
                            .matchedGeometryEffect(id: "tab-pill", in: tabPill)
                    }
                }
                .contentShape(Circle())
    }
}

/// The height `.tabBarBottomInset()` reserves — exposed for the one layout
/// that has to subtract it (the Agent page's centred empty state).
let tabBarBottomClearance: CGFloat = 80

extension View {
    /// Bottom clearance for the floating MobileTabBar (EXP-36): bar height
    /// (42pt tab frame + 2×5pt pill padding + 8pt top + 4pt bottom = 64pt)
    /// plus 16pt of breathing room. The bar is an ancestor OVERLAY (see
    /// MainNavigator) — ancestor safe-area insets don't reliably reach List
    /// content inside pushed destinations, so every bar-visible scrollable
    /// (Agent root, Issues list, Devices, Inbox's inbox/my-issues, Reviews,
    /// Actions) applies
    /// this ONE modifier directly. Detail screens (showsTabBar == false) must
    /// NOT reserve it — pass `false` when the same scrollable is reused on a
    /// bar-less surface.
    @ViewBuilder
    func tabBarBottomInset(_ enabled: Bool = true) -> some View {
        if enabled {
            safeAreaInset(edge: .bottom) {
                Color.clear.frame(height: tabBarBottomClearance)
            }
        } else {
            self
        }
    }
}
