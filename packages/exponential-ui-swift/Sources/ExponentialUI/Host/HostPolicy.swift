import Foundation
import ExponentialUICore

// VAPP-91: the host API's policy values (`catalog/host.json`). The rules
// themselves live in the Rust core (`decideFunction`, `decideUrlJson`,
// `mediaRequestJson`, `parseSourceJson`); these are typed views of the JSON
// they take.

/// `allow | ask | deny | not_found`.
public enum FunctionDecision: String, Sendable, Codable {
    case allow, ask, deny
    case notFound = "not_found"
}

/// The function gate: exact names or `prefix*` patterns. Deny wins, then
/// allow, then ask, then `default`.
public struct FunctionPolicy: Sendable, Codable, Equatable {
    public var allow: [String]
    public var ask: [String]
    public var deny: [String]
    public var `default`: FunctionDecision

    public init(allow: [String] = [], ask: [String] = [], deny: [String] = [], default: FunctionDecision = .allow) {
        self.allow = allow
        self.ask = ask
        self.deny = deny
        self.default = `default`
    }
}

/// openUrl / Link: allowed schemes (default https, http, mailto, tel),
/// optional hosts (`example.com`, `*.example.com`), the base relative urls
/// resolve against.
public struct UrlPolicy: Sendable, Codable, Equatable {
    public var schemes: [String]?
    public var hosts: [String]?
    public var baseUrl: String?

    public init(schemes: [String]? = nil, hosts: [String]? = nil, baseUrl: String? = nil) {
        self.schemes = schemes
        self.hosts = hosts
        self.baseUrl = baseUrl
    }
}

/// The media loader: relative urls resolve against `baseUrl`; the absolute
/// url must use one of `schemes` (default https, http, data: no `file`
/// unless listed) and, when `hosts` is set, an http(s) host matching one
/// (exact or `*.example.com`); every rule whose `prefix` the url starts
/// with adds its headers.
public struct MediaOptions: Sendable, Codable, Equatable {
    public struct Rule: Sendable, Codable, Equatable {
        public var prefix: String
        public var headers: [String: String]
        public init(prefix: String, headers: [String: String]) {
            self.prefix = prefix
            self.headers = headers
        }
    }

    public var baseUrl: String?
    public var rules: [Rule]
    /// The schemes a src may use (nil = the contract's `media.defaultSchemes`).
    public var schemes: [String]?
    /// http(s) hosts media may load from (nil = any).
    public var hosts: [String]?

    public init(baseUrl: String? = nil, rules: [Rule] = [], schemes: [String]? = nil, hosts: [String]? = nil) {
        self.baseUrl = baseUrl
        self.rules = rules
        self.schemes = schemes
        self.hosts = hosts
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        baseUrl = try c.decodeIfPresent(String.self, forKey: .baseUrl)
        rules = try c.decodeIfPresent([Rule].self, forKey: .rules) ?? []
        schemes = try c.decodeIfPresent([String].self, forKey: .schemes)
        hosts = try c.decodeIfPresent([String].self, forKey: .hosts)
    }
}

/// `media.limits` of the host contract: what every image load must fit.
public struct MediaLimits: Sendable, Equatable {
    public var maxBytes: Int64
    public var timeoutMs: Double
    public var maxPixels: Int64

    public init(maxBytes: Int64, timeoutMs: Double, maxPixels: Int64) {
        self.maxBytes = maxBytes
        self.timeoutMs = timeoutMs
        self.maxPixels = maxPixels
    }

    /// Read ONCE from the core's contract (`hostContractJson()`).
    public static let contract: MediaLimits = {
        let l = JSONValue.parse(hostContractJson())["media"]?["limits"]
        return MediaLimits(maxBytes: Int64(l?["maxBytes"]?.number ?? 0), timeoutMs: l?["timeoutMs"]?.number ?? 0, maxPixels: Int64(l?["maxPixels"]?.number ?? 0))
    }()
}

/// The contract's `paint` section: the hook and the error code a failed
/// painter reports with.
enum PaintContract {
    static let errorCode: String = JSONValue.parse(hostContractJson())["paint"]?["errorCode"]?.string ?? "RENDER_FAILED"
}

/// The href a link may navigate to under `policy` (the absolute url), or
/// nil when the URL policy denies it: paint it as text, never open it.
public func safeHref(_ url: String, policy: UrlPolicy?) -> String? {
    guard !url.isEmpty, let json = try? decideUrlJson(policyJson: PolicyJSON.encode(policy), url: url) else { return nil }
    let d = JSONValue.parse(json)
    guard d["allowed"]?.bool == true, let abs = d["url"]?.string, !abs.isEmpty else { return nil }
    return abs
}

/// The media request for `src` under `options` (nil options = the
/// defaults): the absolute url + the rules' headers, or nil when the media
/// policy denies it (nothing loads).
public func policyMediaRequest(_ src: String, options: MediaOptions?) -> URLRequest? {
    guard !src.isEmpty else { return nil }
    let json = PolicyJSON.encode(options ?? MediaOptions()) ?? "{}"
    guard let out = try? mediaRequestJson(url: src, optionsJson: json), let r = JSONValue.parse(out).object, let u = r["url"]?.string.flatMap(URL.init(string:)) else { return nil }
    var request = URLRequest(url: u)
    for (k, v) in r["headers"]?.object ?? [:] { request.setValue(v.displayText, forHTTPHeaderField: k) }
    return request
}

/// Whether an already-built request's url passes the media schemes / hosts
/// (a host hook can rewrite, never widen).
func mediaAllowed(_ request: URLRequest, options: MediaOptions?) -> Bool {
    guard let url = request.url?.absoluteString else { return false }
    let check = MediaOptions(schemes: options?.schemes, hosts: options?.hosts)
    return policyMediaRequest(url, options: check) != nil
}

/// A parsed binding source `<scheme>:<name>[?k=v&…]`.
public struct BindingSource: Sendable, Equatable {
    public let uri: String
    public let scheme: String
    public let name: String
    public let params: [String: String]

    /// Parse through the core's rule (nil when the uri is not a source).
    public static func parse(_ uri: String) -> BindingSource? {
        guard let json = parseSourceJson(uri: uri), let o = JSONValue.parse(json).object else { return nil }
        var params: [String: String] = [:]
        for (k, v) in o["params"]?.object ?? [:] { params[k] = v.displayText }
        return BindingSource(uri: o["uri"]?.string ?? uri, scheme: o["scheme"]?.string ?? "", name: o["name"]?.string ?? "", params: params)
    }
}

/// One scheme's resolver: subscribe the source; every `emit` writes the
/// value at the bound path (nil removes it). Returns the cancel.
public typealias SourceResolver = @MainActor (_ source: BindingSource, _ emit: @escaping @MainActor (JSONValue?) -> Void) -> (() -> Void)?

/// What a host function receives besides its args.
public typealias FunctionCallInfo = SurfaceFunctionCall

/// A registered host function (an action with effects; may be async).
public typealias HostFunction = @MainActor (_ args: Props, _ call: FunctionCallInfo) async throws -> Any?

/// The policy hooks of a host.
public struct HostPolicy {
    /// The declarative gate; `ask` decisions go to `onFunctionCall`.
    public var functions: FunctionPolicy?
    /// The consent hook: true runs the call. nil = `ask` is denied.
    public var onFunctionCall: (@MainActor (FunctionCallInfo) async -> Bool)?
    public var urls: UrlPolicy?
    /// Opens an allowed url (default: the system opener).
    public var openUrl: (@MainActor (URL) -> Void)?
    public var media: MediaOptions?

    public init(functions: FunctionPolicy? = nil, onFunctionCall: (@MainActor (FunctionCallInfo) async -> Bool)? = nil, urls: UrlPolicy? = nil, openUrl: (@MainActor (URL) -> Void)? = nil, media: MediaOptions? = nil) {
        self.functions = functions
        self.onFunctionCall = onFunctionCall
        self.urls = urls
        self.openUrl = openUrl
        self.media = media
    }
}

/// An extension the host registers: the catalog JSON and a painter per
/// native kind.
public struct HostExtension {
    public let json: String
    public let painters: [String: ExtensionPainter]
    /// The catalog id (`id` of the JSON).
    public let id: String

    public init(json: String, painters: [String: ExtensionPainter] = [:]) {
        self.json = json
        self.painters = painters
        self.id = JSONValue.parse(json)["id"]?.string ?? ""
    }
}

/// The result of a gated function call.
public struct FunctionOutcome {
    public let decision: FunctionDecision
    public let result: Any?
    public let error: String?

    public init(decision: FunctionDecision, result: Any? = nil, error: String? = nil) {
        self.decision = decision
        self.result = result
        self.error = error
    }
}

enum PolicyJSON {
    static func encode<T: Encodable>(_ value: T?) -> String? {
        guard let value, let data = try? JSONEncoder().encode(value) else { return nil }
        return String(decoding: data, as: UTF8.self)
    }
}
