import SwiftUI

/// The Exponential UI kitchen sink as a BLANK app: this project adds the
/// `ExponentialUI` package by local path (`..`) and nothing else, no Tuist,
/// no Exponential code. It is what `shots/exponential-ui-kitchen-sink/
/// ios.webp` is captured from (`-shot exponential-ui-kitchen-sink`).
///
/// Launch arguments: `-theme <exponential|neutral|playful|brand>`,
/// `-mode <light|dark>`, `-rtl`, `-width <N>`, `-overlays painted`,
/// `-shot <view>` (hides the chrome), `-a11yDump` (prints the UIAccessibility
/// walk VoiceOver would read, two seconds after launch).
@main
struct KitchenSinkApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
