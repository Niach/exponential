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
    public static let navigateFunction = "app_navigate"

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

    /// #EXP-1674: the screens the host's navigation paints (no `nav: false`).
    public static func nav(_ pkg: JSONValue) -> [Resolved] {
        (pkg["screens"]?.array ?? []).compactMap { s in
            guard s["nav"]?.bool != false, let template = s["template"]?.string else { return nil }
            return resolve(pkg, screen: template, params: nil)
        }
    }

    /// #EXP-1674: what a link FROM ANOTHER APP may open in this package, or why
    /// it is refused (`host-router.json` `crossApp`). The target (`screen`, else
    /// the first in the navigation) must be `exposed`; its params = the screen's
    /// own, then the request's params the contract declares; a `required` one
    /// must be present and non-empty.
    public static func crossAppTarget(_ pkg: JSONValue, screen: String?, params: [String: String]?) -> Result<NavigateTarget.Target, NavigateTarget.Refusal> {
        let screens = (pkg["screens"]?.array ?? []).filter { $0["template"]?.string != nil }
        let picked: JSONValue?
        if let screen, !screen.isEmpty {
            picked = screens.first { $0["template"]?.string == screen }
        } else {
            picked = screens.first { $0["nav"]?.bool != false }
        }
        guard let picked, let template = picked["template"]?.string else {
            return .failure(NavigateTarget.Refusal(message: "no such screen"))
        }
        guard let exposed = picked["exposed"], exposed.object != nil else {
            return .failure(NavigateTarget.Refusal(message: "\(template) is not exposed"))
        }
        let declared = exposed["params"]?.object ?? [:]
        var merged: [String: String] = [:]
        for (k, p) in picked["params"]?.object ?? [:] { if let s = p.string { merged[k] = s } }
        for (k, v) in params ?? [:] where declared[k] != nil { merged[k] = v }
        for k in declared.keys.sorted() where declared[k]?["required"]?.bool == true {
            if merged[k]?.isEmpty ?? true {
                return .failure(NavigateTarget.Refusal(message: "\(template) needs \(k)"))
            }
        }
        return .success(NavigateTarget.Target(screen: template, params: merged))
    }

    /// The screen a host opens for a request (a deep link, a navigate, the
    /// default): the requested one (a `nav: false` one too), else the first in
    /// the navigation; nil without screens.
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
