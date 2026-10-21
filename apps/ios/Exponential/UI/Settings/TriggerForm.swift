import ExpCore
import ExpUI
import SwiftUI

// EXP-615: the ONE trigger editor — the Schedule | On event switch over the
// when-part of a trigger. NEW-2: triggers live on a team AGENT definition and
// the SERVER fires them: a schedule = a 5-field cron (`m h dom mon dow`) read
// in an IANA time zone (blank = UTC), an event = the name of an `events` row
// (a builtin, contract `eventBuiltinNames`, or any custom name).

/// One trigger's own glyph, by kind (a Triggers row's leading mark).
enum TriggerGlyph {
    static func icon(for trigger: TriggerWhen) -> String {
        trigger.isSchedule ? AppIcons.triggerSchedule : AppIcons.triggerEvent
    }
}

/// The trigger being edited, in field-shaped values.
struct TriggerDraft: Equatable {
    /// The event picker's "type a name" row.
    static let customEvent = "custom"

    var kind = "schedule"
    var cron = "0 9 * * 1-5"
    /// IANA zone; blank = UTC. A new trigger seeds the phone's own zone.
    var tz = TimeZone.current.identifier
    /// A builtin event name, or `customEvent`.
    var eventPick = DomainContract.eventBuiltinNames.first ?? customEvent
    var customName = ""

    init() {}

    /// Seed the fields from an existing trigger.
    init(trigger: TriggerWhen?) {
        switch trigger {
        case let .schedule(s)?:
            kind = "schedule"
            cron = s.cron
            tz = s.tz ?? ""
        case let .event(e)?:
            kind = "event"
            if DomainContract.eventBuiltinNames.contains(e.name) {
                eventPick = e.name
            } else {
                eventPick = Self.customEvent
                customName = e.name
            }
        case nil:
            break
        }
    }

    private var trimmedCron: String {
        cron.trimmingCharacters(in: .whitespacesAndNewlines)
            .split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }

    private var trimmedTz: String { tz.trimmingCharacters(in: .whitespacesAndNewlines) }

    private var eventName: String {
        eventPick == Self.customEvent
            ? customName.trimmingCharacters(in: .whitespacesAndNewlines)
            : eventPick
    }

    /// What the server's strict schema would take: five cron fields within
    /// the contract cap (a known zone or none), or a non-blank event name
    /// within its cap of letters, digits and `. _ : / -`.
    var isValid: Bool {
        if kind == "schedule" {
            let cron = trimmedCron
            guard cron.split(separator: " ").count == 5,
                  cron.count <= DomainContract.agentTriggerCronMax
            else { return false }
            return trimmedTz.isEmpty
                || (trimmedTz.count <= DomainContract.agentTriggerTzMax && TimeZone(identifier: trimmedTz) != nil)
        }
        let name = eventName
        guard let first = name.unicodeScalars.first,
              name.count <= DomainContract.eventNameMax,
              CharacterSet.alphanumerics.contains(first)
        else { return false }
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._:/-")
        return name.unicodeScalars.allSatisfy { allowed.contains($0) }
    }

    /// The configured trigger in wire form.
    var trigger: TriggerWhen {
        if kind == "schedule" {
            return .schedule(ScheduleTrigger(cron: trimmedCron, tz: trimmedTz.isEmpty ? nil : trimmedTz))
        }
        return .event(EventTrigger(name: eventName))
    }
}

/// The when-part `Section` of the trigger form.
struct TriggerForm: View {
    @Binding var draft: TriggerDraft

    @ViewBuilder
    var body: some View {
        // EXP-721 (EXP-698 r4 parity): the Schedule / On event tabs are the
        // FIRST ROW of the trigger card — the embedded strip — not a capsule
        // under a "Trigger" heading.
        Section {
            GlassSegmentedControl(
                options: ["schedule", "event"],
                selection: draft.kind,
                label: { $0 == "schedule" ? "Schedule" : "On event" },
                style: .embedded,
                onSelect: { draft.kind = $0 }
            )
            .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
            // The hairline under the strip starts at the inset its sibling rows'
            // dividers use (the default row inset, 12pt past this row's 8pt),
            // not at the first segment's label.
            .alignmentGuide(.listRowSeparatorLeading) { d in d[.leading] + 12 }
            if draft.kind == "schedule" {
                scheduleRows
            } else {
                eventRows
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    @ViewBuilder
    private var scheduleRows: some View {
        textRow("Cron", placeholder: "0 9 * * 1-5", text: $draft.cron, id: "trigger-cron")
            .font(.system(.body, design: .monospaced))
        textRow("Time zone", placeholder: "UTC", text: $draft.tz, id: "trigger-tz")
    }

    @ViewBuilder
    private var eventRows: some View {
        GlassPickerRow(
            "When",
            selection: $draft.eventPick,
            options: DomainContract.eventBuiltinNames + [TriggerDraft.customEvent],
            label: { value in
                value == TriggerDraft.customEvent
                    ? "Custom event"
                    : AgentSettings.Copy.events[value] ?? value
            }
        )
        if draft.eventPick == TriggerDraft.customEvent {
            textRow("Event", placeholder: "deploy:prod", text: $draft.customName, id: "trigger-event-name")
        }
    }

    /// A labelled single-line text row: the title leading, the field filling
    /// the rest (the picker rows' layout).
    private func textRow(
        _ title: String, placeholder: String, text: Binding<String>, id: String
    ) -> some View {
        GlassTextField(
            placeholder,
            text: text,
            bordered: false,
            accessibilityIdentifier: id,
            leading: {
                Text(title)
                    .foregroundStyle(.white)
                    .frame(minWidth: 88, alignment: .leading)
            },
            trailing: { EmptyView() }
        )
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
    }
}
