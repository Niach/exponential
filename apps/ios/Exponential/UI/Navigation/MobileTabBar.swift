import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1379/#EXP-1384 (chats.json `shell`, ×3 phones): the floating bottom
/// STRIP = the glass TAB CAPSULE (the system Liquid Glass look on iOS 26+,
/// the opaque card composite before it) with `shell.tabs` in order —
/// Board · Chats · Apps · Team — each an icon over its label, the selected
/// one on the glass-active pill; beside it, unchanged, the SPLIT launcher
/// (chat | new issue).
///
/// THE CAPSULE (`shell.bar`) takes the width the screen leaves beside the
/// launcher; each tab starts at its label's width and the tabs share the
/// rest (`MobileTabBarMetrics.tabWidths`), so the board's and the team's
/// names read whole. THE BOARD TAB = the active board's glyph + name (the
/// `nav-issues` concept + "Board" while the team has none); on the board
/// view it wears the `nav-team-switcher` caret and a tap opens THE BOARD
/// MENU: the whole strip MORPHS into one glass panel listing the team's
/// boards (then New board), the phone's only board switch. DRAG
/// (`shell.bar.drag`, the native tab bar's behaviour): a press lifts the
/// LENS under the finger (iOS 26: interactive Liquid Glass), it follows the
/// finger along the bar, the tab under its centre is the candidate, a lift
/// selects it (the same as a tap), a lift farther than `cancelDistance`
/// off the bar cancels.
///
/// Attached via `.overlay(alignment: .bottom)` so content scrolls underneath
/// it; each bar-visible scrollable reserves clearance with
/// `.tabBarBottomInset()` (EXP-36). MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    /// The lit tab (nil = none: the agent home is up).
    var selected: Chats.Shell.Tab?
    /// Lit while the agent home is up: the chat arm wears the selected circle.
    var agentActive = false
    /// The ACTIVE board (the Board tab's glyph + name); nil = none.
    var board: BoardEntity?
    /// The board view is up: the Board tab wears the caret and opens the menu.
    var onBoardView = false
    /// The active team (the Team tab's avatar and label); nil = none yet.
    var team: TeamEntity?
    /// The Chats dot: amber = a chat needs me, green = one runs, primary =
    /// one holds news I have not read.
    var chatsRunning = false
    var chatsNeedYou = false
    var chatsUnread = false
    /// EXP-973: whether the New-issue arm can go anywhere.
    var composeEnabled = true
    /// The board menu's rows: the active team's boards in sidebar order.
    var boards: [BoardEntity] = []
    /// The viewer may create boards: the muted New board row.
    var onNewBoard: (() -> Void)?
    var onSelect: (Chats.Shell.Tab) -> Void = { _ in }
    var onSelectBoard: (BoardEntity) -> Void = { _ in }
    var onCompose: () -> Void = {}
    var onChat: () -> Void = {}
    /// Styleguide only: a static state the specimen photographs.
    var specimen: Specimen?

    enum Specimen: Equatable {
        /// The board menu drawn open over the strip it grew from.
        case menuOpen
        /// The lens lifted mid-drag over `candidate`, `offset` pt off its centre.
        case lens(candidate: Chats.Shell.Tab, offset: CGFloat)
    }

    // EXP-523: the selected pill SLIDES between tabs.
    @Namespace private var tabPill
    @Environment(\.motion) private var motion
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// The press in progress (nil = none).
    @State private var press: Press?
    /// The lens: under the finger while pressed, settling after a lift.
    @State private var lens: Lens?
    /// The tab under the lens' centre while pressed.
    @State private var candidate: Chats.Shell.Tab?
    /// Every tab's frame in the strip's coordinate space.
    @State private var tabFrames: [Chats.Shell.Tab: CGRect] = [:]
    /// The board menu: mounted through its collapse; `menuExpanded` drives
    /// the morph (false = the strip's frame, true = the menu's).
    @State private var menuMounted = false
    @State private var menuExpanded = false

    private struct Press {
        let pressed: Chats.Shell.Tab
        var dragging = false
    }

    private struct Lens: Equatable {
        var minX: CGFloat
        var width: CGFloat
        var scale: CGFloat
    }

    private static let space = "phone-tab-strip"

    var body: some View {
        ZStack(alignment: .bottom) {
            if menuMounted && specimen == nil {
                // The transparent scrim: a tap outside the panel collapses it.
                Color.black.opacity(0.001)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { closeMenu() }
                    .accessibilityHidden(true)
            }
            strip
        }
        .onChange(of: onBoardView) { _, onBoard in
            // Leaving the board view folds the menu.
            if !onBoard {
                menuMounted = false
                menuExpanded = false
            }
        }
        .onAppear {
            if specimen == .menuOpen {
                menuMounted = true
                menuExpanded = true
            }
        }
    }

    // MARK: - Strip

    private var strip: some View {
        HStack(spacing: MobileTabBarMetrics.launcherGap) {
            tabCapsule
            launcherCapsule
        }
        .opacity(menuExpanded ? 0 : 1)
        .allowsHitTesting(!menuExpanded)
        .accessibilityHidden(menuExpanded)
        .overlay(alignment: .bottom) {
            if menuMounted { boardMenu }
        }
        .padding(.horizontal, MobileTabBarMetrics.inset)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private var tabCapsule: some View {
        TabStripLayout {
            ForEach(Chats.Shell.tabs, id: \.self) { tab in
                tabFace(tab)
            }
        }
        .coordinateSpace(.named(Self.space))
        .background(alignment: .topLeading) { lensView }
        .onPreferenceChange(TabFramesKey.self) { frames in
            MainActor.assumeIsolated { tabFrames = frames }
        }
        .contentShape(Rectangle())
        .gesture(dragGesture)
        .animation(motion.standard, value: selected)
        .padding(MobileTabBarMetrics.pillPadding)
        .frame(maxWidth: .infinity)
        .tabBarGlass()
        .onChange(of: tabFrames) { _, frames in
            // The specimen's lens, once the tabs have been measured.
            if case let .lens(tab, offset) = specimen, let frame = frames[tab] {
                candidate = tab
                lens = Lens(minX: frame.minX + offset, width: frame.width, scale: lensScale)
            }
        }
    }

    // MARK: - Tabs

    private var lensScale: CGFloat { reduceMotion ? 1 : MobileTabBarMetrics.lensScale }

    /// While pressed the candidate is lit; else the selected tab.
    private func isLit(_ tab: Chats.Shell.Tab) -> Bool {
        lens != nil && candidate != nil ? candidate == tab : selected == tab
    }

    private func label(_ tab: Chats.Shell.Tab) -> String {
        switch tab {
        case .board: board?.name ?? tab.label
        case .team: team?.name ?? tab.label
        default: tab.label
        }
    }

    private func identifier(_ tab: Chats.Shell.Tab) -> String {
        switch tab {
        case .board: Chats.TestIds.tabBoard
        // The Chats tab keeps the id every suite already reaches it by.
        case .chats: "tab-threads"
        case .apps: "tab-apps"
        case .team: Chats.TestIds.tabTeam
        }
    }

    private var showsCaret: Bool { onBoardView && board != nil }

    private func tabFace(_ tab: Chats.Shell.Tab) -> some View {
        let lit = isLit(tab)
        let text = label(tab)
        return VStack(spacing: 4) {
            tabIcon(tab)
                .frame(height: 22)
                .overlay(alignment: .topTrailing) {
                    if tab == .chats, chatsRunning || chatsNeedYou || chatsUnread {
                        Circle()
                            .fill(chatsNeedYou
                                ? DesignTokens.Semantic.yellow
                                : chatsRunning ? DesignTokens.Semantic.green : DesignTokens.Palette.primary)
                            .frame(width: 8, height: 8)
                            .offset(x: 5, y: -1)
                    }
                }
            HStack(spacing: MobileTabBarMetrics.caretGap) {
                Text(text)
                    .font(.system(size: MobileTabBarMetrics.labelSize, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.tail)
                if tab == .board && showsCaret {
                    AppIcon(AppIcons.navTeamSwitcher, size: MobileTabBarMetrics.caretSize, weight: .semibold)
                        .fixedSize()
                        .layoutPriority(1)
                }
            }
        }
        .foregroundStyle(.white.opacity(lit ? 1 : TextOpacity.secondary))
        .padding(.horizontal, MobileTabBarMetrics.tabPaddingX)
        .frame(minWidth: MobileTabBarMetrics.tabMinWidth, maxWidth: .infinity)
        .frame(height: MobileTabBarMetrics.tabHeight)
        .background {
            // Only the SELECTED tab renders the pill, carrying the shared
            // geometry id — that is what makes it travel. A press lifts it
            // (the lens takes over).
            if selected == tab && lens == nil {
                Capsule()
                    .fill(GlassTokens.fillActive)
                    .matchedGeometryEffect(id: "tab-pill", in: tabPill)
            }
        }
        .background(GeometryReader { proxy in
            Color.clear.preference(
                key: TabFramesKey.self,
                value: [tab: proxy.frame(in: .named(Self.space))]
            )
        })
        .contentShape(Capsule())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
        .accessibilityAddTraits(selected == tab ? [.isButton, .isSelected] : .isButton)
        .accessibilityAction { activate(tab) }
        .accessibilityIdentifier(identifier(tab))
    }

    @ViewBuilder
    private func tabIcon(_ tab: Chats.Shell.Tab) -> some View {
        switch tab {
        case .board:
            if let board {
                AppIcon(BoardTypeDisplay.iconName(for: board), size: AppIcon.Size.large)
                    .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
            } else {
                AppIcon(AppIcons.navIssues, size: AppIcon.Size.large)
            }
        case .team:
            if let team {
                TeamAvatar(team: team, size: 20)
            } else {
                AppIcon(AppIcons.navSettings, size: AppIcon.Size.large)
            }
        case .chats:
            AppIcon(AppIcons.navThreads, size: AppIcon.Size.large)
        case .apps:
            AppIcon(AppIcons.navApps, size: AppIcon.Size.large)
        }
    }

    // MARK: - Drag lens

    @ViewBuilder
    private var lensView: some View {
        if let lens {
            LensFace()
                .frame(width: lens.width, height: MobileTabBarMetrics.tabHeight)
                .scaleEffect(lens.scale)
                .offset(x: lens.minX)
                .allowsHitTesting(false)
        }
    }

    private var dragGesture: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .named(Self.space))
            .onChanged { value in
                if press == nil {
                    guard specimen == nil, let pressed = tab(at: value.startLocation.x),
                          let frame = tabFrames[pressed] else { return }
                    press = Press(pressed: pressed)
                    candidate = pressed
                    // The press lifts the selection under the finger.
                    withAnimation(motion.fast) {
                        lens = Lens(minX: frame.minX, width: frame.width, scale: lensScale)
                    }
                }
                guard var current = press else { return }
                if !current.dragging {
                    let moved = hypot(value.translation.width, value.translation.height)
                    guard moved >= MobileTabBarMetrics.dragSlop else { return }
                    current.dragging = true
                    press = current
                }
                let next = candidateAt(value.location.x)
                candidate = next.tab
                // The lens moves WITH the finger: no animation while dragging.
                var still = Transaction()
                still.disablesAnimations = true
                withTransaction(still) {
                    lens = Lens(minX: next.minX, width: next.width, scale: lensScale)
                }
            }
            .onEnded { value in
                guard let current = press else { return }
                press = nil
                guard current.dragging else {
                    // A plain tap selects at once.
                    settle(on: current.pressed)
                    activate(current.pressed)
                    return
                }
                let height = MobileTabBarMetrics.tabHeight
                let y = value.location.y
                let off = y < 0 ? -y : (y > height ? y - height : 0)
                let target = candidateAt(value.location.x).tab
                if off > MobileTabBarMetrics.cancelDistance || target == nil {
                    // A lift far off the bar cancels: back to the selected tab.
                    settle(on: selected)
                } else if let target {
                    settle(on: target)
                    activate(target)
                }
            }
    }

    /// The tab whose frame holds `x`, else the nearest one.
    private func tab(at x: CGFloat) -> Chats.Shell.Tab? {
        if let hit = tabFrames.first(where: { $0.value.minX <= x && x <= $0.value.maxX }) {
            return hit.key
        }
        return tabFrames.min { lhs, rhs in
            distance(x, lhs.value) < distance(x, rhs.value)
        }?.key
    }

    private func distance(_ x: CGFloat, _ frame: CGRect) -> CGFloat {
        min(abs(x - frame.minX), abs(x - frame.maxX))
    }

    /// The candidate under a finger at `x` and the lens frame centred on the
    /// finger (clamped to the bar).
    private func candidateAt(_ x: CGFloat) -> (tab: Chats.Shell.Tab?, minX: CGFloat, width: CGFloat) {
        let lower = tabFrames.values.map(\.minX).min() ?? 0
        let upper = tabFrames.values.map(\.maxX).max() ?? 0
        let clamped = min(max(x, lower), upper)
        let tab = tab(at: clamped)
        let width = tab.flatMap { tabFrames[$0]?.width } ?? MobileTabBarMetrics.tabMinWidth
        let minX = min(max(clamped - width / 2, lower), upper - width)
        return (tab, minX, width)
    }

    /// The lens settles over `tab` at standard / decelerate, then leaves the
    /// selected pill in charge.
    private func settle(on tab: Chats.Shell.Tab?) {
        candidate = nil
        guard let tab, let frame = tabFrames[tab] else {
            lens = nil
            return
        }
        let target = Lens(minX: frame.minX, width: frame.width, scale: 1)
        if let animation = motion.decelerate(DesignTokens.Motion.Duration.standard) {
            withAnimation(animation) {
                lens = target
            } completion: {
                if press == nil { lens = nil }
            }
        } else {
            lens = nil
        }
    }

    /// A tab's selection — a tap, a lift, a VoiceOver activation. On the board
    /// view the Board tab opens the board menu instead (`shell.boardTab`).
    private func activate(_ tab: Chats.Shell.Tab) {
        if tab == .board && showsCaret {
            menuMounted ? closeMenu() : openMenu()
        } else {
            onSelect(tab)
        }
    }

    // MARK: - Board menu

    private var menuRowCount: Int { boards.count + (onNewBoard == nil ? 0 : 1) }

    private var openAnimation: Animation {
        motion.decelerate(DesignTokens.Motion.Duration.slow)
            ?? .linear(duration: DesignTokens.Motion.Duration.standard)
    }

    private var closeAnimation: Animation {
        motion.accelerate(DesignTokens.Motion.Duration.standard)
            ?? .linear(duration: DesignTokens.Motion.Duration.standard)
    }

    private func openMenu() {
        menuMounted = true
        // One frame at the strip's frame first, then the morph.
        DispatchQueue.main.async {
            withAnimation(openAnimation) { menuExpanded = true }
        }
    }

    private func closeMenu(then follow: (() -> Void)? = nil) {
        withAnimation(closeAnimation) {
            menuExpanded = false
        } completion: {
            if !menuExpanded { menuMounted = false }
            follow?()
        }
    }

    /// The panel grows from the strip's frame (same left, right and bottom
    /// edges; the corner radius from the strip's to `radius`) while the
    /// strip fades out and the rows fade in. Reduce Motion: a cross-fade.
    private var boardMenu: some View {
        let open = reduceMotion || menuExpanded
        let height = open
            ? MobileTabBarMetrics.menuHeight(rows: menuRowCount)
            : MobileTabBarMetrics.barHeight
        let radius = open ? MobileTabBarMetrics.menuRadius : MobileTabBarMetrics.barHeight / 2
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        return ScrollView {
            VStack(spacing: 0) {
                ForEach(boards, id: \.id) { row in
                    boardRow(row)
                }
                if let onNewBoard {
                    Button {
                        closeMenu(then: onNewBoard)
                    } label: {
                        HStack(spacing: 12) {
                            AppIcon(AppIcons.uiAdd, size: 16)
                                .frame(width: 20, height: 20)
                            Text(Chats.Copy.BoardMenu.newBoard)
                                .font(.system(size: 15))
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 16)
                        .frame(height: MobileTabBarMetrics.menuRowHeight)
                        .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(Chats.TestIds.boardMenuRow)
                }
            }
            .padding(MobileTabBarMetrics.pillPadding)
            .opacity(menuExpanded ? 1 : 0)
        }
        .scrollDisabled(menuRowCount <= MobileTabBarMetrics.menuMaxVisibleRows)
        .scrollBounceBehavior(.basedOnSize)
        .frame(maxWidth: .infinity)
        .frame(height: height, alignment: .top)
        .clipShape(shape)
        .menuGlass(in: shape)
        .opacity(reduceMotion ? (menuExpanded ? 1 : 0) : 1)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Chats.TestIds.boardMenu)
    }

    private func boardRow(_ row: BoardEntity) -> some View {
        let active = row.id == board?.id
        return Button {
            closeMenu()
            if !active { onSelectBoard(row) }
        } label: {
            HStack(spacing: 12) {
                AppIcon(BoardTypeDisplay.iconName(for: row), size: 20)
                    .foregroundStyle(Color(hex: row.color ?? "#888888") ?? .gray)
                    .frame(width: 20, height: 20)
                Text(row.name)
                    .font(.system(size: 15))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                if active {
                    AppIcon(AppIcons.uiCheck, size: 16)
                        .foregroundStyle(.white)
                }
            }
            .padding(.horizontal, 16)
            .frame(height: MobileTabBarMetrics.menuRowHeight)
            .background {
                if active { Capsule().fill(GlassTokens.fillActive) }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(row.name)
        .accessibilityAddTraits(active ? .isSelected : [])
        .accessibilityIdentifier(Chats.TestIds.boardMenuRow)
    }

    // MARK: - Launcher

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
                            .frame(width: MobileTabBarMetrics.tabHeight, height: MobileTabBarMetrics.tabHeight)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// The drag lens: iOS 26 = the interactive Liquid Glass; before it the
/// glass-active pill.
private struct LensFace: View {
    var body: some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer {
                Capsule()
                    .fill(GlassTokens.fillActive)
                    .glassEffect(.regular.interactive(), in: Capsule())
            }
        } else {
            Capsule().fill(GlassTokens.fillActive)
        }
    }
}

/// The tabs' frames in the strip's coordinate space.
private struct TabFramesKey: PreferenceKey {
    static let defaultValue: [Chats.Shell.Tab: CGRect] = [:]
    static func reduce(value: inout [Chats.Shell.Tab: CGRect], nextValue: () -> [Chats.Shell.Tab: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

/// `shell.bar`: each tab at its natural width (label + padding, at least the
/// minimum), the rest shared evenly; the widest gives way first when the
/// labels alone do not fit (`MobileTabBarMetrics.tabWidths`).
private struct TabStripLayout: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let natural = subviews.map { $0.sizeThatFits(.unspecified).width }
        return CGSize(
            width: proposal.width ?? natural.reduce(0, +),
            height: MobileTabBarMetrics.tabHeight
        )
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let natural = subviews.map { $0.sizeThatFits(.unspecified).width }
        let widths = MobileTabBarMetrics.tabWidths(natural: natural, available: bounds.width)
        var x = bounds.minX
        for (subview, width) in zip(subviews, widths) {
            subview.place(
                at: CGPoint(x: x, y: bounds.minY),
                anchor: .topLeading,
                proposal: ProposedViewSize(width: width, height: MobileTabBarMetrics.tabHeight)
            )
            x += width
        }
    }
}

private extension View {
    /// The board menu's chrome: THE glass menu fill (GlassMenu's opaque card
    /// composite + its white tint, so the page never reads through the rows)
    /// under the strong hairline.
    func menuGlass<S: Shape>(in shape: S) -> some View {
        self
            .background(GlassTokens.opaqueCardFill, in: shape)
            .overlay(shape.fill(Color.white.opacity(GlassMenuTokens.tintOpacity)).allowsHitTesting(false))
            .overlay(shape.stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
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
    /// (the 52pt capsule + 8pt top + 4pt bottom = 64pt)
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
