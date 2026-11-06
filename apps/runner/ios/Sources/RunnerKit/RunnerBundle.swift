// #VAPP-268: the bundle the runner reads (runner.json `bundle`): ONE
// directory `app/` in the app's resources (`<name>.app/app`) holding the
// package file and `runner.json`. Nothing else is read at run time and
// nothing comes from the network.
import Foundation
import ExponentialEngine

/// Why the bundle cannot run (runner.json `failureCases`): painted with
/// `runnerFailure(kind:)` in place of the app. `busy` is the web's (another
/// tab holds the store); the iOS runner never paints it.
public enum RunnerFailureKind: String, CaseIterable, Error, Sendable {
    case missing, config, signature, invalid, busy
}

/// `runner.json` once `runnerConfig` passed it (runner.json `config.keys`).
public struct RunnerConfig: Equatable, Sendable {
    public var id: String
    public var name: String
    public var version: String
    /// `off` | `upstreamsOnly` | `any`.
    public var network: String

    public init(id: String, name: String, version: String, network: String) {
        self.id = id; self.name = name; self.version = version; self.network = network
    }

    /// The Engine's network policy for `network`.
    public var networkPolicy: NetworkPolicy {
        switch network {
        case "any": return .any
        case "upstreamsOnly": return .upstreamsOnly
        default: return .off
        }
    }
}

/// The bundle read from `app/`: its validated config and the package file's text.
public struct RunnerBundle: Equatable, Sendable {
    public static let dir = "app"
    public static let packageFile = "package.app.json"
    public static let configFile = "runner.json"

    public var config: RunnerConfig
    /// The package FILE (`{package, author, signature}`, or a bare package).
    public var packageJson: String

    /// `<name>.app/app` (runner.json `bundle.locations.ios`).
    public static var location: URL? {
        Bundle.main.resourceURL?.appendingPathComponent(dir, isDirectory: true)
    }

    /// runner.json first (absent, not JSON or any issue = `config`), then
    /// the package file (absent = `missing`). The package itself is the
    /// Engine's to judge (`signature`, `invalid`).
    public static func read(_ dir: URL?) -> Result<RunnerBundle, RunnerFailureKind> {
        guard let dir else { return .failure(.missing) }
        let fm = FileManager.default
        guard let configData = fm.contents(atPath: dir.appendingPathComponent(configFile).path),
              let config = parseConfig(String(decoding: configData, as: UTF8.self)) else { return .failure(.config) }
        guard let packageData = fm.contents(atPath: dir.appendingPathComponent(packageFile).path) else { return .failure(.missing) }
        return .success(RunnerBundle(config: config, packageJson: String(decoding: packageData, as: UTF8.self)))
    }

    /// `runnerConfig(json)` → the config, nil on any issue (or not JSON).
    public static func parseConfig(_ json: String) -> RunnerConfig? {
        guard let checked = try? runnerConfig(configJson: json),
              let object = RunnerJSON.object(checked),
              (object["issues"] as? [Any])?.isEmpty == true,
              let config = object["config"] as? [String: Any],
              let id = config["id"] as? String, let name = config["name"] as? String,
              let version = config["version"] as? String, let network = config["network"] as? String else { return nil }
        return RunnerConfig(id: id, name: name, version: version, network: network)
    }
}

/// JSON text in and out (the Engine's types cross as JSON).
public enum RunnerJSON {
    public static func value(_ text: String) -> Any? {
        try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed])
    }

    public static func object(_ text: String) -> [String: Any]? { value(text) as? [String: Any] }

    public static func text(_ value: Any) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .fragmentsAllowed, .withoutEscapingSlashes]) else { return "null" }
        return String(decoding: data, as: UTF8.self)
    }
}
