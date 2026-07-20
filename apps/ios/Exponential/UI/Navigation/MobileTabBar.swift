import ExpUI
import SwiftUI

/// Linear-style floating bottom navigation: a glass pill with the top-level
/// destinations (Issues, My Work — with an unread dot — Support — the team
/// helpdesk inbox, present only while the active team's helpdesk flag is on
/// (EXP-180) — Agents — with a running-session dot — Reviews — its own entry
/// per EXP-147, ordered after Agents per EXP-152 — and Search; base order per
/// EXP-81) plus a detached circular compose button on the right. Attached via
/// `.overlay(alignment: .bottom)` so content
/// scrolls underneath it; each bar-visible scrollable reserves clearance with
/// `.tabBarBottomInset()` (EXP-36). MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    let issuesActive: Bool
    let searchActive: Bool
    let agentsActive: Bool
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
    let onIssues: () -> Void
    let onSearch: () -> Void
    let onAgents: () -> Void
    let onMyWork: () -> Void
    let onReviews: () -> Void
    let onSupport: () -> Void
    let onCompose: () -> Void

    /// SF Symbols has no robot-head glyph, so the Agents tab draws a bundled
    /// template vector asset; every other tab keeps a system symbol.
    private enum TabGlyph {
        case system(String)
        case asset(String)
    }

    var body: some View {
        HStack(spacing: 12) {
            // Six tabs (helpdesk on) must still fit a 375pt screen (SE/mini)
            // beside the compose circle: drop the inter-tab spacing and pull
            // the outer padding in — the 44pt touch targets stay intact.
            HStack(spacing: showsSupport ? 0 : 4) {
                tab(glyph: .system("list.bullet"), label: "Issues", active: issuesActive, action: onIssues)
                    .accessibilityIdentifier("tab-issues")
                // EXP-58: the Inbox tab became My Work (Inbox + My Issues
                // merged) — same glyph, same unread dot.
                tab(
                    glyph: .system("tray"),
                    label: "My Work",
                    active: myWorkActive,
                    badge: unreadCount > 0,
                    badgeColor: Accent.indigo,
                    action: onMyWork
                )
                .accessibilityIdentifier("tab-mywork")
                // Support (EXP-180): the team helpdesk inbox — the web
                // sidebar's LifeBuoy entry. Present only while the active
                // team's synced helpdesk flag is on.
                if showsSupport {
                    tab(
                        glyph: .system("lifepreserver"),
                        label: "Support",
                        active: supportActive,
                        badge: supportUnread,
                        badgeColor: Accent.indigo,
                        action: onSupport
                    )
                    .accessibilityIdentifier("tab-support")
                }
                tab(
                    glyph: .asset("tab-robot"),
                    label: "Agents",
                    active: agentsActive,
                    badge: agentsRunning,
                    // Amber while any session waits on a plan approval /
                    // question (EXP-214), live green otherwise.
                    badgeColor: agentsNeedInput
                        ? DesignTokens.Semantic.yellow
                        : DesignTokens.Semantic.green,
                    action: onAgents
                )
                .accessibilityIdentifier("tab-agents")
                // Reviews sits beside Agents (EXP-147/EXP-152) — the same
                // open-PR glyph the in_review status uses. Green dot while
                // open PRs await review (EXP-214).
                tab(
                    glyph: .system("arrow.triangle.pull"),
                    label: "Reviews",
                    active: reviewsActive,
                    badge: reviewsOpen,
                    badgeColor: DesignTokens.Semantic.green,
                    action: onReviews
                )
                .accessibilityIdentifier("tab-reviews")
                tab(glyph: .system("magnifyingglass"), label: "Search", active: searchActive, action: onSearch)
                    .accessibilityIdentifier("tab-search")
            }
            .padding(5)
            .background(.ultraThinMaterial, in: Capsule())
            .overlay(
                Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)

            Spacer()

            if showsCompose {
                Button(action: onCompose) {
                    Image(systemName: "square.and.pencil")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(.white)
                        .frame(width: 52, height: 52)
                        .background(.ultraThinMaterial, in: Circle())
                        .overlay(
                            Circle().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                        )
                        .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("New issue")
                .accessibilityIdentifier("compose-button")
            }
        }
        .padding(.horizontal, showsSupport ? 12 : 20)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private func tab(
        glyph: TabGlyph,
        label: String,
        active: Bool,
        badge: Bool = false,
        badgeColor: Color = Accent.indigo,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            glyphImage(glyph, active: active)
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
                .background(active ? Color.white.opacity(0.12) : .clear, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    @ViewBuilder
    private func glyphImage(_ glyph: TabGlyph, active: Bool) -> some View {
        switch glyph {
        case let .system(name):
            Image(systemName: name)
                .font(.body.weight(.medium))
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
        case let .asset(name):
            Image(name)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: 20, height: 20)
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
        }
    }
}

extension View {
    /// Bottom clearance for the floating MobileTabBar (EXP-36): bar height
    /// (42pt tab frame + 2×5pt pill padding + 8pt top + 4pt bottom = 64pt)
    /// plus 16pt of breathing room. The bar is an ancestor OVERLAY (see
    /// MainNavigator) — ancestor safe-area insets don't reliably reach List
    /// content inside pushed destinations, so every bar-visible scrollable
    /// (Issues list, Search results, Agents, My Work's inbox/my-issues,
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
