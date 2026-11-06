import ActivityKit
import Foundation

// #EXP-1639 (live-activity.json): THE attributes type of a session's Live
// Activity, compiled into the app and the widget extension. push-relay starts
// it by NAME (`"attributes-type": "SessionActivityAttributes"`) with empty
// `attributes`, so it has no stored properties; the content-state is ONLY the
// sealed blob `s` (the server sealed `{title, body, data}` to this install's
// push key), opened on the device by `LiveActivityDisplay.open`.
struct SessionActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        /// The sealed content, base64url (push-seal.json).
        var s: String
    }
}
