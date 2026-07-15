import Foundation

public enum AppConstants {
    public static let publicCloudUrl = "https://app.exponential.at"
    public static let stagingCloudUrl = "https://next.exponential.at"

    /// Whether this build targets the staging cloud. Detected from the host
    /// bundle id (the staging app/extension bundles carry `.staging`) rather than
    /// a compile-time `#if STAGING`, so it resolves correctly from inside the
    /// once-compiled `ExpCore` framework regardless of which app links it.
    public static var isStaging: Bool {
        Bundle.main.bundleIdentifier?.contains(".staging") ?? false
    }

    public static var defaultCloudUrl: String {
        isStaging ? stagingCloudUrl : publicCloudUrl
    }

    /// This build's marketing version, read from the app's Info.plist
    /// (`CFBundleShortVersionString`, injected from `appMarketingVersion` in
    /// Project.swift). Falls back to `0.0.0` so a missing/malformed plist reads
    /// as the oldest possible version rather than crashing.
    public static var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0"
    }

    /// The `x-client-version` header value every request carries (EXP-104):
    /// `ios/<marketingVersion>`. The server uses it to gate builds below the
    /// configured minimum with a 426 (`client_upgrade_required`).
    public static var clientVersionHeaderValue: String {
        "ios/\(appVersion)"
    }

    /// The App Store listing, opened from the blocking Update-required view.
    // TODO: real App Store id once published
    public static let appStoreUrl = URL(string: "https://apps.apple.com/app/id0000000000")!
}
