import ActivityKit
import ExpPush
import SwiftUI
import WidgetKit

// #EXP-1639 (live-activity.json): the widget extension hosting a session's
// Live Activity. push-relay starts / updates / ends it over APNs with a
// content-state of ONLY the sealed blob; this opens it with the push keys in
// the shared keychain (the ones the Notification Service Extension uses) and
// draws it. Nothing is fetched: what does not open shows the generic title.

@main
struct SessionLiveActivityBundle: WidgetBundle {
    var body: some Widget {
        SessionLiveActivity()
        // #EXP-1830 (home-widgets.json): the home screen widgets.
        BoardHomeWidget()
        MyIssuesHomeWidget()
        NeedsYouHomeWidget()
    }
}

struct SessionLiveActivity: Widget {
    private static func display(_ state: SessionActivityAttributes.ContentState) -> LiveActivityDisplay {
        LiveActivityDisplay.open(sealed: state.s, keys: PushKeyStore().loadAll())
    }

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: SessionActivityAttributes.self) { context in
            let display = Self.display(context.state)
            SessionActivityLockScreen(display: display)
                .activityBackgroundTint(DesignTokens.Palette.background)
                .activitySystemActionForegroundColor(DesignTokens.Palette.foreground)
                .widgetURL(display.deepLink)
        } dynamicIsland: { context in
            let display = Self.display(context.state)
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    SessionActivityMark(size: 36)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    SessionActivityBadge(display: display)
                        .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    SessionActivityExpanded(display: display)
                        .padding(.horizontal, 4)
                }
            } compactLeading: {
                SessionActivityMark(size: 22)
            } compactTrailing: {
                SessionActivityBadge(display: display, compact: true)
            } minimal: {
                SessionActivityMark(size: 22)
            }
            .widgetURL(display.deepLink)
            .keylineTint(DesignTokens.Palette.foreground)
        }
    }
}
