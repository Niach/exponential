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
struct AppPreview: View {
    /// The package's JSON text; nil = no version yet.
    let packageJSON: String?
    /// VAPP-122: the hosting app and its team (`exp:records` reads and
    /// `harness.records.*` writes the app's own rows).
    var appId: String?
    var teamId: String?
    /// Release train 2026-10-25: the row's grants are about the HEAD
    /// package; an older version's preview runs with none (web alike).
    var grants = true

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Apps.Preview.radius)
        ScrollView {
            Group {
                if let packageJSON {
                    AppPackageSurface(packageJSON: packageJSON, appId: appId, teamId: teamId, grants: grants)
                        .id("\(grants):\(packageJSON)")
                } else {
                    EmptyState(title: Apps.Copy.previewEmpty, icon: AppIcons.navApps, size: .page)
                        .frame(maxWidth: .infinity, minHeight: 192)
                }
            }
            .padding(Apps.Preview.padding)
            .frame(maxWidth: .infinity, alignment: .topLeading)
        }
        .scrollContentBackground(.hidden)
        .background(GlassTokens.backgroundTop, in: shape)
        .clipShape(shape)
        .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.preview)
    }
}

/// The live surface of one package: its own host, the first template
/// applied once, closed when it leaves. Without the painter linked: nothing
/// (the frame stays).
private struct AppPackageSurface: View {
    let packageJSON: String
    let appId: String?
    let teamId: String?
    let grants: Bool

    #if canImport(ExponentialUI)
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var appHost: ExpAppHost?
    @State private var consent = AppConsentGate()

    var body: some View {
        Group {
            if let appHost {
                HostSurface(host: appHost.host, surfaceId: Apps.Preview.surfaceId)
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
            onOpenIssue: { [pushRoute, accountId] issueId in
                pushRoute(.issue(accountId: accountId, id: issueId))
            }
        )
        if let templateId = Apps.firstTemplateId(packageJSON: packageJSON) {
            host.apply(surfaceId: Apps.Preview.surfaceId, packageId: packageId, templateId: templateId)
        }
        appHost = host
    }
    #else
    var body: some View { EmptyView() }
    #endif
}
