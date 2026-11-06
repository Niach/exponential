// #VAPP-268: the iOS runner app (the TEMPLATE make-project.sh builds; not a
// SwiftPM target). It has NO chrome: the window is the genUI surfaces the
// Engine builds (app, consent, permissions, toast), painted by ExponentialUI.
// No network for the package, no OTA, no telemetry, no account: the bundle
// is `<name>.app/app`, the identity `local`, the Vault the Keychain.
import SwiftUI
import ExponentialEngine
import ExponentialUI
import RunnerKit

@main
struct RunnerApp: App {
    @UIApplicationDelegateAdaptor(RunnerAppDelegate.self) private var delegate
    @Environment(\.scenePhase) private var phase
    @State private var model = RunnerModel.shared

    var body: some Scene {
        WindowGroup {
            RunnerView(model: model)
                .onOpenURL { model.session.open(url: $0) }
        }
        .onChange(of: phase) { _, phase in
            switch phase {
            case .active: model.session.scene(.foreground)
            case .background: model.session.scene(.background)
            default: break
            }
        }
    }
}

final class RunnerAppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        // Only when the notifications module is linked (RunnerCapabilities.swift, generated).
        RunnerCapabilities.installTaps { json in RunnerModel.shared.session.notificationTap(json) }
        MainActor.assumeIsolated { RunnerModel.shared.session.start() }
        return true
    }

    func applicationWillTerminate(_ application: UIApplication) {
        RunnerModel.shared.session.scene(.terminate)
        RunnerModel.shared.session.settle()
    }
}

/// The window: surface `app` fills it; the sheets and the toast paint above
/// it (their own Drawer / Toast layers), hit-testable only while a modal
/// layer is open.
struct RunnerView: View {
    let model: RunnerModel
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            ZStack(alignment: .topLeading) {
                surface(RunnerSurface.app, size)
                surface(RunnerSurface.permissions, size)
                surface(RunnerSurface.consent, size)
                surface(RunnerSurface.toast, size).allowsHitTesting(false)
            }
            .onChange(of: size.width, initial: true) { _, width in model.width(width) }
        }
        .background((model.host.theme?.color("background", mode: scheme == .dark ? .dark : .light) ?? Color(.systemBackground)).ignoresSafeArea())
    }

    @ViewBuilder
    func surface(_ id: String, _ size: CGSize) -> some View {
        if let surface = model.host.surface(id) {
            HostSurface(host: model.host, surfaceId: id)
                .frame(width: size.width, height: size.height, alignment: .topLeading)
                .allowsHitTesting(id == RunnerSurface.app || surface.layers.contains { $0.modal })
                .onAppear { surface.setViewport(width: size.width, height: size.height) }
                .onChange(of: size) { _, size in surface.setViewport(width: size.width, height: size.height) }
        }
    }
}
