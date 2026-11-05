import Foundation

// #VAPP-229 (packages/domain-contract/fixtures/via-app.json): the "via <app>"
// caption of every row an app caused (issues, sessions, records; comments
// read reporter-reply.json's `captions.viaApp`, the same string). Mirrors the
// TS reference packages/domain-contract/src/via-app.ts; ViaAppTests replays
// the fixture's captionCases and appendCases.
public enum ViaApp {
    /// Fixture `copy.viaApp`; `{app}` = the resolved app's name.
    public static let captionTemplate = "via {app}"
    /// Fixture `copy.separator`.
    public static let separator = " · "
    /// VAPP-231, fixture `claim.header`: the MCP header a claim rides on.
    public static let header = "X-Exp-Via-App"
}

/// VAPP-231 (fixture `claim`): a host's claim that a write comes from its app
/// (`viaApp` on a records write); the server checks it, it only adds the caption.
public struct ViaAppClaim: Encodable, Equatable, Sendable {
    public let appId: String
    public let version: Int

    public init(appId: String, version: Int) {
        self.appId = appId
        self.version = version
    }

    /// The MCP header's value, `{appId}@{version}`.
    public var headerValue: String { "\(appId)@\(version)" }
}

/// The claim of a host over `appId`; nil without an app or a published version.
public func viaAppClaim(_ appId: String?, publishedVersion: Int?) -> ViaAppClaim? {
    guard let appId, !appId.isEmpty, let publishedVersion, publishedVersion > 0 else { return nil }
    return ViaAppClaim(appId: appId, version: publishedVersion)
}

/// "via {app}", or nil without a (resolved, non-empty) app name.
public func viaAppCaption(_ appName: String?) -> String? {
    guard let appName, !appName.isEmpty else { return nil }
    return ViaApp.captionTemplate.replacingOccurrences(of: "{app}", with: appName)
}

/// `text` with the caption appended after the separator; the text alone
/// without an app.
public func withViaApp(_ text: String, _ appName: String?) -> String {
    guard let caption = viaAppCaption(appName) else { return text }
    return text + ViaApp.separator + caption
}
