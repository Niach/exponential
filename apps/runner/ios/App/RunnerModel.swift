// #VAPP-268: the runner's PAINTER: one ExponentialHost (genUI's SwiftUI host)
// whose surfaces are runner.json's `app`, `consent`, `permissions` and
// `toast`. Every function the app calls and every `records:` source it binds
// go to the RunnerSession (the Engine instance); the runner's own events
// (`runner.*`, `consent.*`) too. Icons = the Lucide registry web draws.
import Foundation
import SwiftUI
import ExponentialEngine
import ExponentialUI
import ExponentialUIPrimitives
import RunnerKit

final class Weak<T: AnyObject> {
    weak var value: T?
}

struct RunnerRefused: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

@MainActor
@Observable
final class RunnerModel: RunnerPainter {
    static let shared = RunnerModel()

    @ObservationIgnored let session: RunnerSession
    let host: ExponentialHost
    @ObservationIgnored private var packageId: String?
    @ObservationIgnored private var sources: [UUID: (uri: String, emit: @MainActor (JSONValue?) -> Void)] = [:]

    init() {
        let bundle = RunnerBundle.read(RunnerBundle.location)
        let config = try? bundle.get().config
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
        let session = RunnerSession(
            bundle: bundle, dataRoot: support,
            vault: KeychainVault(service: config?.id ?? Bundle.main.bundleIdentifier ?? "runner"),
            capabilities: RunnerCapabilities.all(),
            host: RunnerSession.Host(platform: "ios", formFactor: UIDevice.current.userInterfaceIdiom == .pad ? "tablet" : "phone"),
            formFactor: UIScreen.main.bounds.width >= 768 ? .wide : .compact)
        self.session = session
        // The host's closures reach the model through a box (they exist before it).
        let me = Weak<RunnerModel>()
        var functions: [String: HostFunction] = [:]
        for name in RunnerFunctions.all {
            functions[name] = { args, _ in try await me.value?.call(name, args) }
        }
        host = ExponentialHost(HostOptions(
            functions: functions,
            sources: ["records": { source, emit in me.value?.bind(source.uri, emit) }],
            settings: SurfaceSettings(mode: .system),
            plugin: ClosureHost(
                icons: { name, size in
                    guard let paths = LucideIcons.paths(name) else { return nil }
                    return AnyView(LucideGlyph(paths: paths, size: size))
                },
                actions: { event in
                    // runner.navigate, runner.permissions, consent.toggle, consent.answer.
                    _ = session.handleEvent(name: event.name, context: event.context.any as? [String: Any] ?? [:])
                }
            ),
            // app.navigate from a component: the Engine opens it (WindowHost.present → open).
            navigate: { target in
                let json = RunnerJSON.text(["screen": target.screen, "params": target.params])
                Task { _ = await session.call(name: "app.navigate", argsJson: json) }
                return true
            }
        ))
        me.value = self
        session.painter = self
    }

    func call(_ name: String, _ args: Props) async throws -> Any? {
        switch await session.call(name: name, argsJson: JSONValue.object(args).json) {
        case let .value(json): return RunnerJSON.value(json)
        case let .refused(message): throw RunnerRefused(message: message)
        }
    }

    func bind(_ uri: String, _ emit: @escaping @MainActor (JSONValue?) -> Void) -> (() -> Void)? {
        let key = UUID()
        sources[key] = (uri, emit)
        resolve(uri, emit)
        return { [weak self] in self?.sources[key] = nil }
    }

    func resolve(_ uri: String, _ emit: @escaping @MainActor (JSONValue?) -> Void) {
        session.recordsValue(source: uri) { value in
            MainActor.assumeIsolated { emit(value.map { JSONValue.parse($0) }) }
        }
    }

    /// The window's width: `wide` from genUI's `md` breakpoint up.
    func width(_ width: CGFloat) {
        let breakpoint = host.surface(RunnerSurface.app)?.breakpoint
        let wide = breakpoint.map { ["md", "lg", "xl"].contains($0) } ?? (width >= 768)
        session.setFormFactor(wide ? .wide : .compact)
    }

    // MARK: RunnerPainter (on the main queue)

    nonisolated func install(packageJson: String) {
        MainActor.assumeIsolated {
            host.installPackage(packageJson)
            packageId = RunnerJSON.object(packageJson)?["id"] as? String
        }
    }

    nonisolated func open(screen: String, paramsJson: String) {
        MainActor.assumeIsolated {
            var apply: [String: Any] = ["surfaceId": RunnerSurface.app, "templateId": screen, "data": ["params": RunnerJSON.value(paramsJson) ?? [:]]]
            if let packageId { apply["packageId"] = packageId }
            host.receive(RunnerJSON.text(["version": ExponentialHost.a2uiVersion, "applyTemplate": apply]))
        }
    }

    nonisolated func paint(surface: String, componentsJson: String, dataJson: String?) {
        MainActor.assumeIsolated {
            let v = ExponentialHost.a2uiVersion
            host.receive(RunnerJSON.text(["version": v, "createSurface": ["surfaceId": surface, "catalogId": SurfaceOptions().catalogId]]))
            host.receive(RunnerJSON.text(["version": v, "updateComponents": ["surfaceId": surface, "components": RunnerJSON.value(componentsJson) ?? []]]))
            if let dataJson, let data = RunnerJSON.value(dataJson) {
                host.receive(RunnerJSON.text(["version": v, "updateDataModel": ["surfaceId": surface, "path": "/", "value": data]]))
            }
        }
    }

    nonisolated func close(surface: String) {
        MainActor.assumeIsolated {
            host.receive(RunnerJSON.text(["version": ExponentialHost.a2uiVersion, "deleteSurface": ["surfaceId": surface]]))
        }
    }

    nonisolated func recordsChanged() {
        MainActor.assumeIsolated {
            for (_, source) in sources { resolve(source.uri, source.emit) }
        }
    }
}
