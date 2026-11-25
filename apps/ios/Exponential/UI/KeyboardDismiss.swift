import SwiftUI
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

extension View {
    /// A chat area that does not scroll: a tap or a drag puts the keyboard
    /// away, as scrolling a transcript does.
    func dismissesKeyboard() -> some View {
        contentShape(Rectangle())
            .onTapGesture { UIApplication.endEditing() }
            .simultaneousGesture(DragGesture(minimumDistance: 10).onChanged { drag in
                if drag.translation.height > 10 { UIApplication.endEditing() }
            })
    }
}
