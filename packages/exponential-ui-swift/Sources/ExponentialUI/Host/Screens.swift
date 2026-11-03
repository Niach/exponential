import Foundation
import ExponentialUICore

/// VAPP-218 (eApps U1, spec `docs/spec/apps.md` §5.1): a package's
/// SCREENS, the navigation manifest (`@exponential_app/eui`
/// `src/host/screens.ts`, eui-core `host::screens`, `fixtures/host-router.json`
/// `screens` + `navigate`). The HOST draws the navigation from it; the
/// `app.navigate {screen, params?}` host function opens one, its params
/// landing under `/params/*` in the template data.
public enum Screens {
    /// The host function (and action call) that opens a screen; always
    /// allowed (the package need not list it).
    public static let navigateFunction = "app.navigate"

    /// A screen as a host opens it.
    public struct Resolved: Equatable, Sendable {
        /// The template it opens (= the screen's id in links).
        public let screen: String
        public let title: String
        public let icon: String?
        public let params: [String: String]

        /// The template data it opens with: `{params}`.
        public var data: JSONValue { .object(["params": .object(params.mapValues { .string($0) })]) }
    }

    /// The screens a (valid) package declares, in order ([] = none: the
    /// host shows the first template as before).
    public static func list(_ pkg: JSONValue) -> [Resolved] {
        (pkg["screens"]?.array ?? []).compactMap { s in
            guard let template = s["template"]?.string else { return nil }
            return resolve(pkg, screen: template, params: nil)
        }
    }

    /// The screen a host opens for a request (a deep link, a navigate, the
    /// default): the requested one, else the first; nil without screens.
    public static func resolve(_ pkg: JSONValue, screen: String?, params: [String: String]?) -> Resolved? {
        let paramsJson = params.map { JSONValue.object($0.mapValues { .string($0) }).json }
        let out = JSONValue.parse((try? resolveScreenJson(packageJson: pkg.json, screen: screen, paramsJson: paramsJson)) ?? "null")
        return resolved(out)
    }

    static func resolved(_ v: JSONValue) -> Resolved? {
        guard let screen = v["screen"]?.string else { return nil }
        var params: [String: String] = [:]
        for (k, p) in v["params"]?.object ?? [:] { if let s = p.string { params[k] = s } }
        return Resolved(screen: screen, title: v["title"]?.string ?? "", icon: v["icon"]?.string, params: params)
    }

    /// An `app.navigate` call's args → its target, or why it is refused.
    public static func target(_ pkg: JSONValue, args: Props) -> Result<NavigateTarget.Target, NavigateTarget.Refusal> {
        let out = JSONValue.parse((try? navigateTargetJson(packageJson: pkg.json, argsJson: JSONValue.object(args).json)) ?? #"{"error":"navigate takes {screen, params?}"}"#)
        if let error = out["error"]?.string { return .failure(NavigateTarget.Refusal(message: error)) }
        var params: [String: String] = [:]
        for (k, p) in out["params"]?.object ?? [:] { if let s = p.string { params[k] = s } }
        return .success(NavigateTarget.Target(screen: out["screen"]?.string ?? "", params: params))
    }
}

/// What `HostOptions.navigate` receives: the surface, its package, the
/// screen and the params.
public struct NavigateTarget: Equatable, Sendable {
    public struct Target: Equatable, Sendable {
        public let screen: String
        public let params: [String: String]
    }
    public struct Refusal: Error, Equatable, Sendable {
        public let message: String
    }
    public let surfaceId: String
    public let packageId: String
    public let screen: String
    public let params: [String: String]

    public init(surfaceId: String, packageId: String, screen: String, params: [String: String]) {
        self.surfaceId = surfaceId
        self.packageId = packageId
        self.screen = screen
        self.params = params
    }
}
