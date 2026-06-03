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
}
