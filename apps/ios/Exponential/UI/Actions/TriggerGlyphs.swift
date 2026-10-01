import ExpCore
import ExpUI
import SwiftUI

/// SLOP-2: the trigger glyphs beside an action's NAME (and a suggestion's
/// title): `trigger-schedule` when it has a schedule trigger, `trigger-event`
/// when it has an event one, both when both. A kind none of whose triggers is
/// enabled draws muted. No text, no count — the action page's Triggers tab
/// says the rest.
struct TriggerGlyphs: View {
    let badges: TriggerBadges
    var size: CGFloat = 12

    var body: some View {
        if !badges.isEmpty {
            HStack(spacing: 4) {
                if let schedule = badges.schedule {
                    glyph(AppIcons.triggerSchedule, active: schedule.active, label: "Schedule trigger")
                }
                if let event = badges.event {
                    glyph(AppIcons.triggerEvent, active: event.active, label: "Event trigger")
                }
            }
            .accessibilityIdentifier("trigger-glyphs")
        }
    }

    private func glyph(_ icon: String, active: Bool, label: String) -> some View {
        AppIcon(icon, size: size)
            .foregroundStyle(.white.opacity(active ? TextOpacity.secondary : TextOpacity.quaternary))
            .accessibilityLabel(active ? label : "\(label), paused")
    }
}

/// One trigger's own glyph, by kind (a Triggers row's leading mark).
enum TriggerGlyph {
    static func icon(for trigger: AutomationTrigger) -> String {
        trigger.isSchedule ? AppIcons.triggerSchedule : AppIcons.triggerEvent
    }
}
