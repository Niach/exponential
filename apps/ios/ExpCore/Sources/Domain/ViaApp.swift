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
