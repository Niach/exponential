import Foundation

enum AppConstants {
    static let publicCloudUrl = "https://app.exponential.at"
    static let stagingCloudUrl = "https://next.exponential.at"

    static var isStaging: Bool {
        #if STAGING
        return true
        #else
        return false
        #endif
    }

    static var defaultCloudUrl: String {
        isStaging ? stagingCloudUrl : publicCloudUrl
    }
}
