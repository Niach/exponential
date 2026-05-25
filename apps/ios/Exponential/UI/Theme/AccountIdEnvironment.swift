import SwiftUI

private struct AccountIdKey: EnvironmentKey {
    static let defaultValue: String = ""
}

extension EnvironmentValues {
    var accountId: String {
        get { self[AccountIdKey.self] }
        set { self[AccountIdKey.self] = newValue }
    }
}
