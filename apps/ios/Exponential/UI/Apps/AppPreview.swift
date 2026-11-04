import ExpCore
import ExpUI
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
/// package's `app.navigate` the call's screen + params (in memory). A new
/// screen = a fresh surface opening its template with `{params}`.
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
    /// `app.navigate` write it), nil = the first.
    var screen: Binding<Apps.ScreenRequest?> = .constant(nil)
    /// VAPP-242: false = THE RUN VIEW: the surface fills the content area
    /// edge to edge (no frame radius, ground or hairline; `preview.padding`
    /// inside), the same host and screens bar.
    var framed = true
    /// VAPP-242: the run view's empty state (`copy.runEmpty`, the app's glyph).
    var emptyTitle = Apps.Copy.previewEmpty
    var emptyIcon = AppIcons.navApps

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: framed ? Apps.Preview.radius : 0)
        let screens = AppPreviewScreens(packageJSON: packageJSON, request: screen.wrappedValue)
        VStack(spacing: 0) {
            ScrollView {
                Group {
                    if let packageJSON {
                        AppPackageSurface(
                            packageJSON: packageJSON, appId: appId, teamId: teamId, grants: grants, readOnly: readOnly,
                            migrateFrom: migrateFrom, shown: screens.shown,
                            onNavigate: { request in screen.wrappedValue = request }
                        )
                        .id("\(grants):\(readOnly):\(migrateFrom ?? ""):\(screens.shownKey):\(packageJSON)")
                    } else {
                        EmptyState(title: emptyTitle, icon: emptyIcon, size: .page)
                            .frame(maxWidth: .infinity, minHeight: 192)
                    }
                }
                .padding(Apps.Preview.padding)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            .scrollContentBackground(.hidden)
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
        .background(framed ? GlassTokens.backgroundTop : Color.clear, in: shape)
        .clipShape(shape)
        .overlay(shape.strokeBorder(framed ? GlassTokens.strokeCard : Color.clear, lineWidth: GlassTokens.hairline))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(framed ? Apps.TestIds.preview : Apps.TestIds.run)
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

    /// The surface's identity part: a new screen (or params) = a fresh surface.
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

/// The live surface of one package: its own host, the first template (or
/// VAPP-218 the shown screen's, with its `{params}`) applied once, closed
/// when it leaves. Without the painter linked: nothing (the frame stays).
private struct AppPackageSurface: View {
    let packageJSON: String
    let appId: String?
    let teamId: String?
    let grants: Bool
    let readOnly: Bool
    let migrateFrom: String?
    /// VAPP-218: the shown screen (nil = no screens).
    let shown: Apps.ScreenRequest?
    /// VAPP-218: the package's `app.navigate` (the request it names).
    let onNavigate: (Apps.ScreenRequest) -> Void

    #if canImport(ExponentialUI)
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var appHost: ExpAppHost?
    @State private var consent = AppConsentGate()

    var body: some View {
        Group {
            if let appHost {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(appHost.upstreams.banners) { banner in
                        upstreamBanner(banner)
                    }
                    HostSurface(host: appHost.host, surfaceId: Apps.Preview.surfaceId)
                }
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .modifier(AppConsentCard(gate: consent))
        .onAppear(perform: build)
        .onDisappear {
            appHost?.close()
            appHost = nil
        }
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
                .font(.caption2)
            Spacer(minLength: 8)
            if let teamId, banner.connect {
                Button(AppUpstream.connect) {
                    pushRoute(.teamSettingsSection(accountId: accountId, teamId: teamId, section: .mcp))
                }
                .font(.caption2.weight(.semibold))
                .buttonStyle(.plain)
            }
        }
        .foregroundStyle(.white.opacity(0.9))
        .padding(.horizontal, 12)
        .padding(.vertical, 5)
        .frame(maxWidth: .infinity)
        .background(.orange.opacity(0.35))
        .background(.ultraThinMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppUpstream.bannerTestId)
    }

    private func build() {
        guard appHost == nil else { return }
        let packageId = JSONValue.parse(packageJSON)["id"]?.string ?? ""
        let host = ExpAppHost(
            packages: [packageJSON],
            appId: appId,
            teamId: teamId,
            me: deps.auth.accounts.first { $0.id == accountId }?.userId ?? deps.auth.userId,
            accountId: accountId,
            deps: deps,
            consent: consent,
            grants: grants,
            readOnly: readOnly,
            migrateFrom: migrateFrom,
            navigate: { [onNavigate] target in
                onNavigate(Apps.ScreenRequest(screen: target.screen, params: target.params))
                return true
            }
        )
        if let shown {
            // `Screens.Resolved.data`: the template opens with `{params}`.
            let params: [String: JSONValue] = shown.params.mapValues { .string($0) }
            host.apply(
                surfaceId: Apps.Preview.surfaceId, packageId: packageId, templateId: shown.screen,
                data: .object(["params": .object(params)])
            )
        } else if let templateId = Apps.previewTemplate(packageJSON: packageJSON) {
            host.apply(surfaceId: Apps.Preview.surfaceId, packageId: packageId, templateId: templateId)
        }
        appHost = host
    }
    #else
    var body: some View { EmptyView() }
    #endif
}
