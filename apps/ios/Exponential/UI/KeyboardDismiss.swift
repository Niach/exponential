import UIKit

extension UIApplication {
    /// Resign whatever holds the keyboard, app-wide. The markdown editors'
    /// UITextViews are UIKit first responders invisible to SwiftUI focus
    /// state, so dismissal must go through the responder chain — used by the
    /// tap-outside catchers and before presenting sheets over a focused
    /// editor, where a stale responder left the formatting strip floating
    /// over unrelated screens (EXP-246).
    static func endEditing() {
        shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }
}
