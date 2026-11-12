import ExpCore
import ExpUI
import OSLog
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif

/// VAPP-110: the editor's PREVIEW (web `app-preview.tsx`): the package's
/// first template rendered through the app host (`ExpAppHost`: `exp:`
/// bindings over GRDB, `harness.*` behind the consent card, the URL policy)
/// in a frame `preview.radius` / `preview.padding` on the theme's ground. A
/// new package = a new host (the identity), so every synced upsert repaints;
/// no package yet = the page-size empty state in the frame.
///
/// VAPP-218 (apps.json `screens`): a package that declares SCREENS paints
/// them as the phone BAR (`Apps.screenShell`: phones are always `phone`)
/// under the surface inside the frame: THE phone-tab-bar capsule
/// (`PhoneTabCapsule`), one tab per screen (its eUI icon, none / unknown =
/// `ui-icon-placeholder`, and its title), the shown one selected. The shown
/// screen = eUI `Screens.resolve` of `screen` (the requested one, else the
/// first); a tab tap requests that screen with its own params, the
/// package's `app_navigate` the call's screen + params (in memory). A new
/// screen = a fresh surface opening its template with `{params}`.
///
/// #VAPP-251 (apps.json A PACKAGE THAT NO LONGER VALIDATES): a package the
/// host refuses (`Apps.surfaceState`) = the page-size `copy.needsUpdate` alone,
/// centred, without the bar; a republished package (a new key) tries again.
///
/// #VAPP-252: the surface's Dialog and Drawer layers escape the scroller and
/// cover the whole screen, tab bar included, in the preview as in the run
/// view (web's fixed overlay, Android's dialog windows): `LiftedLayersCover`.
/// The run view's scroller runs to the screen's bottom edge, under the home
/// indicator, when no bar stands there.
struct AppPreview: View {
    /// The package's JSON text; nil = no version yet.
    let packageJSON: String?
    /// VAPP-122: the hosting app and its team (`records:<kind>` reads and
    /// `records.*` writes the app's own rows).
    var appId: String?
    var teamId: String?
    /// Release train 2026-10-25: the row's grants are about the HEAD
    /// package; an older version's preview runs with none (web alike).
    var grants = true
    /// VAPP-200: the viewer's app access is `read`: the app's own writing
    /// controls paint disabled (eUI `readOnly`).
    var readOnly = false
    /// eApps R8: a draft preview's published `version` (its records read
    /// through the package's migrations, copies only); nil = as stored.
    var migrateFrom: String? = nil
    /// VAPP-218: the requested screen (the editor's state; a tab tap and
    /// `app_navigate` write it), nil = the first.
    var screen: Binding<Apps.ScreenRequest?> = .constant(nil)
    /// VAPP-242: false = THE RUN VIEW: the surface fills the content area
    /// edge to edge (no frame radius, ground or hairline; `preview.padding`
    /// inside), the same host and screens bar.
    var framed = true
    /// VAPP-242: the run view's empty state (`copy.runEmpty`, the app's glyph).
    var emptyTitle = Apps.Copy.previewEmpty
    var emptyIcon = AppIcons.navApps
    /// #VAPP-251: the app's glyph on `copy.needsUpdate`.
    var glyph = AppIcons.navApps
    /// #VAPP-251: the surface key the host refused (a new key = a new try).
    @State private var refusedKey: String?
    /// The scroller's visible height: the surface's viewport, what its
    /// Dialogs and Drawers size against (`setViewport`); the surface itself
    /// is as tall as its content.
    @State private var visibleHeight: CGFloat = 0

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: framed ? Apps.Preview.radius : 0)
        let screens = AppPreviewScreens(packageJSON: packageJSON, request: screen.wrappedValue)
        let hostKey = "\(grants):\(readOnly):\(migrateFrom ?? ""):\(packageJSON ?? "")"
        let surfaceKey = "\(screens.shownKey):\(hostKey)"
        VStack(spacing: 0) {
            if refusedKey == surfaceKey {
                EmptyState(title: Apps.Copy.needsUpdate, icon: glyph, size: .page)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
            // #FEED-201: the bar's screens page like every phone tab strip
            // (THE pager, `FacePager`; a neighbour mounts as it comes into
            // reach); a detail screen (`nav: false`) stands alone.
            if let shown = screens.shown, screens.tabs.contains(where: { $0.screen == shown.screen }),
               Apps.screenShell(screenCount: screens.tabs.count, phone: true) == .bar {
                FacePager(
                    pages: screens.tabs.map(\.screen),
                    selection: Binding(
                        get: { shown.screen },
                        set: { next in if next != shown.screen { screen.wrappedValue = Apps.ScreenRequest(screen: next) } }
                    )
                ) { tab in
                    let page = tab == shown.screen ? screens : AppPreviewScreens(packageJSON: packageJSON, request: Apps.ScreenRequest(screen: tab))
                    surfaceScroller(page, surfaceKey: "\(page.shownKey):\(hostKey)", hostKey: hostKey)
                }
            } else {
                surfaceScroller(screens, surfaceKey: surfaceKey, hostKey: hostKey)
            }
            if Apps.screenShell(screenCount: screens.tabs.count, phone: true) == .bar {
                PhoneTabCapsule(
                    tabs: screens.tabs.map(\.screen),
                    selected: screens.shown?.screen,
                    label: { screens.title($0) },
                    identifier: { "\(Apps.TestIds.previewScreens)-\($0)" },
                    onActivate: { tapped in screen.wrappedValue = Apps.ScreenRequest(screen: tapped) }
                ) { tab in
                    VStack(spacing: 4) {
                        AppIcon(screens.icon(tab), size: AppIcon.Size.large)
                            .frame(height: 22)
                        Text(screens.title(tab))
                            .font(.system(size: MobileTabBarMetrics.labelSize, weight: .medium))
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .padding(.horizontal, Apps.Preview.padding)
                .padding(.bottom, Apps.Preview.padding)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(Apps.TestIds.previewScreens)
            }
            }
        }
        .background(framed ? GlassTokens.backgroundTop : Color.clear, in: shape)
        .modifier(FrameClip(shape: shape, framed: framed))
        .overlay(shape.strokeBorder(framed ? GlassTokens.strokeCard : Color.clear, lineWidth: GlassTokens.hairline))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(framed ? Apps.TestIds.preview : Apps.TestIds.run)
    }
}

extension AppPreview {
    /// One screen's surface in its scroller (the pager's page, or the whole
    /// pane without a pager).
    @ViewBuilder
    fileprivate func surfaceScroller(_ page: AppPreviewScreens, surfaceKey: String, hostKey: String) -> some View {
        ScrollView {
            Group {
                if let packageJSON {
                    AppPackageSurface(
                        packageJSON: packageJSON, appId: appId, teamId: teamId, grants: grants, readOnly: readOnly,
                        migrateFrom: migrateFrom, shown: page.shown, shownKey: page.shownKey,
                        viewportHeight: max(0, visibleHeight - Apps.Preview.padding * 2),
                        onNavigate: { request in screen.wrappedValue = request },
                        onRefused: { refusedKey = surfaceKey }
                    )
                    .id(hostKey)
                } else {
                    EmptyState(title: emptyTitle, icon: emptyIcon, size: .page)
                        .frame(maxWidth: .infinity, minHeight: 192)
                }
            }
            .padding(Apps.Preview.padding)
            .frame(maxWidth: .infinity, alignment: .topLeading)
        }
        .scrollContentBackground(.hidden)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { visibleHeight = $0 }
    }
}

/// The editor's frame clips to its radius; the run view has no frame, so
/// nothing stops its scroller at the safe area.
private struct FrameClip: ViewModifier {
    let shape: RoundedRectangle
    let framed: Bool

    func body(content: Content) -> some View {
        if framed {
            content.clipShape(shape)
        } else {
            content
        }
    }
}

/// VAPP-218: a package's screens (eUI `Screens.list`) and the one shown
/// for a request (`Screens.resolve`); none without the painter linked.
private struct AppPreviewScreens {
    struct Tab {
        let screen: String
        let title: String
        let icon: String?
    }

    private(set) var tabs: [Tab] = []
    /// The shown screen with its params (nil = no screens: the first template).
    private(set) var shown: Apps.ScreenRequest?

    init(packageJSON: String?, request: Apps.ScreenRequest?) {
        #if canImport(ExponentialUI)
        guard let packageJSON else { return }
        let pkg = JSONValue.parse(packageJSON)
        // #EXP-1674: the bar paints the navigation's; a `nav: false` screen opens by link.
        tabs = Screens.nav(pkg).map { Tab(screen: $0.screen, title: $0.title, icon: $0.icon) }
        guard !tabs.isEmpty else { return }
        shown = Screens.resolve(pkg, screen: request?.screen, params: request?.params)
            .map { Apps.ScreenRequest(screen: $0.screen, params: $0.params) }
        #endif
    }

    /// The screen's identity: a new screen (or params) = a fresh surface.
    var shownKey: String {
        guard let shown else { return "" }
        let params = shown.params.keys.sorted().map { "\($0)=\(shown.params[$0] ?? "")" }.joined(separator: "&")
        return "\(shown.screen)?\(params)"
    }

    func title(_ screen: String) -> String {
        tabs.first { $0.screen == screen }?.title ?? screen
    }

    /// The screen's eUI icon by name; none / unknown = `ui-icon-placeholder`.
    func icon(_ screen: String) -> String {
        tabs.first { $0.screen == screen }?.icon.flatMap(AppIcons.conceptOrName) ?? AppIcons.uiIconPlaceholder
    }
}

#if canImport(ExponentialUI)
/// #VAPP-252: the surface's lifted layers in a clear `.fullScreenCover` (THE
/// surface that paints over a navigation bar, a tab bar and the keyboard, as
/// `GlassAlert`'s), presented and dismissed animation-free: the layers bring
/// their own enter, and the cover stays for their exit. A consent card a
/// layer's press asks for hangs off the cover (one presentation per level).
private struct LiftedLayersCover: ViewModifier {
    let model: SurfaceModel?
    let consent: AppConsentGate
    @State private var shown = false

    private var open: Bool { model?.liftedLayersOpen ?? false }

    private func setShown(_ value: Bool) {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { shown = value }
    }

    func body(content: Content) -> some View {
        content
            .onChange(of: open) { _, open in
                if open {
                    setShown(true)
                    return
                }
                Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(140))
                    if !self.open { setShown(false) }
                }
            }
            .fullScreenCover(isPresented: Binding(get: { shown }, set: { setShown($0) })) {
                Group {
                    if let model {
                        ExponentialSurfaceLayers(model: model)
                            .modifier(AppConsentCard(gate: consent, onCover: true))
                            .onAppear { consent.covered = true }
                            .onDisappear { consent.covered = false }
                    }
                }
                .presentationBackground(.clear)
            }
            .transaction(value: shown) { $0.disablesAnimations = true }
    }
}
#endif

/// The live surface of one package: its own host, the first template (or
/// VAPP-218 the shown screen's, with its `{params}`) on a minted id, a new
/// screen on a new id on the same host (round 9 §1.4), closed when it leaves.
/// Without the painter linked: nothing (the frame stays).
private struct AppPackageSurface: View {
    let packageJSON: String
    let appId: String?
    let teamId: String?
    let grants: Bool
    let readOnly: Bool
    let migrateFrom: String?
    /// VAPP-218: the shown screen (nil = no screens).
    let shown: Apps.ScreenRequest?
    /// The shown screen's identity: a new one re-opens the surface.
    let shownKey: String
    /// The height the surface's overlay layers size against.
    let viewportHeight: CGFloat
    /// VAPP-218: the package's `app_navigate` (the request it names).
    let onNavigate: (Apps.ScreenRequest) -> Void
    /// #VAPP-251: the host holds no surface after the apply.
    let onRefused: () -> Void

    #if canImport(ExponentialUI)
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var appHost: ExpAppHost?
    @State private var consent = AppConsentGate()
    /// The painted surface: minted per open, so it is state.
    @State private var surfaceId: String?

    private var model: SurfaceModel? { surfaceId.flatMap { appHost?.host.surfaces[$0] } }

    var body: some View {
        Group {
            if let appHost {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(appHost.upstreams.banners) { banner in
                        upstreamBanner(banner)
                    }
                    if let surfaceId {
                        HostSurface(host: appHost.host, surfaceId: surfaceId)
                    }
                }
                .onChange(of: surfaceId, initial: true) { _, _ in syncViewport() }
                .onChange(of: viewportHeight) { _, _ in syncViewport() }
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .modifier(AppConsentCard(gate: consent))
        .modifier(LiftedLayersCover(model: model, consent: consent))
        .task { await build() }
        .onChange(of: shownKey) { _, _ in
            if let appHost { open(appHost) }
        }
        .onDisappear {
            appHost?.close()
            appHost = nil
        }
    }

    private func syncViewport() {
        guard let model else { return }
        model.setViewport(width: model.width, height: viewportHeight, maxHeight: model.maxHeight)
    }

    /// #VAPP-230: the viewer has no connection to an upstream the app reads
    /// (`AppUpstream.bannerServers`): "Connect <server>" as the amber notice
    /// of ×4 (upstream.json `binding`: `SyncHealthBanner`'s tone, the
    /// warning icon, the text, one text action opening Settings › MCP).
    /// #VAPP-239: an offline device's notice is the same row without the action.
    private func upstreamBanner(_ banner: AppUpstream.HostBanner) -> some View {
        HStack(spacing: 6) {
            AppIcon(AppIcons.uiWarning, size: 11)
            Text(banner.text)
                .font(.caption)
            Spacer(minLength: 8)
            if let teamId, banner.connect {
                Button(AppUpstream.connect) {
                    pushRoute(.teamSettingsSection(accountId: accountId, teamId: teamId, section: .mcp))
                }
                .font(.caption.weight(.semibold))
                .buttonStyle(.plain)
            }
        }
        .foregroundStyle(DesignTokens.Semantic.yellow)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity)
        .background(DesignTokens.Semantic.yellow.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(DesignTokens.Semantic.yellow.opacity(0.4), lineWidth: 1))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppUpstream.bannerTestId)
    }

    /// #PERF-5: the team's zone and slug are read off main before the host builds.
    private func build() async {
        guard appHost == nil else { return }
        let zone = await AppHostStore.teamTimeZone(teamId: teamId, pool: deps.db.poolIfOpen(forAccountId: accountId))
        let slug = await LinkOpener.loadTeamSlug(teamId: teamId, accountId: accountId, deps: deps)
        guard appHost == nil, !Task.isCancelled else { return }
        let host = ExpAppHost(
            packages: [packageJSON],
            appId: appId,
            teamId: teamId,
            me: deps.auth.accounts.first { $0.id == accountId }?.userId ?? deps.auth.userId,
            accountId: accountId,
            deps: deps,
            consent: consent,
            teamZone: zone,
            linkBase: AppLink.expandTeamLink("~/", team: slug),
            grants: grants,
            readOnly: readOnly,
            migrateFrom: migrateFrom,
            navigate: { [onNavigate] target in
                onNavigate(Apps.ScreenRequest(screen: target.screen, params: target.params))
                return true
            }
        )
        appHost = host
        surfaceId = nil
        open(host)
    }

    /// Round 9 §1.4: the shown screen onto a minted id, repainted under it,
    /// then the old surface goes.
    private func open(_ host: ExpAppHost) {
        let packageId = JSONValue.parse(packageJSON)["id"]?.string ?? ""
        let base = Apps.Preview.surfaceId
        if let shown {
            // `Screens.Resolved.data`: the template opens with `{params}`.
            let params: [String: JSONValue] = shown.params.mapValues { .string($0) }
            surfaceId = host.reopen(
                current: surfaceId, base: base, packageId: packageId, templateId: shown.screen,
                data: .object(["params": .object(params)])
            )
        } else if let templateId = Apps.previewTemplate(packageJSON: packageJSON) {
            surfaceId = host.reopen(current: surfaceId, base: base, packageId: packageId, templateId: templateId)
        }
        if Apps.surfaceState(hasPackage: true, applied: true, hasSurface: surfaceId.flatMap { host.host.surface($0) } != nil) == .needsUpdate {
            // The validator's words stay in the log, never on screen.
            let issues = host.host.installPackage(packageJSON).map(\.json).joined(separator: "; ")
            Logger(subsystem: "at.exponential", category: "apps").error("app \(appId ?? "?", privacy: .public) paints nothing: \(issues, privacy: .public)")
            onRefused()
        }
    }
    #else
    var body: some View { EmptyView() }
    #endif
}
