import Foundation

// #EXP-1850 (apps.json `eventsTab`; TS reference `src/apps.ts` `eventRunRow`,
// desktop `domain::apps`, Android `domain/Apps.kt`): an app's EVENTS tab =
// the app agent's trigger list, the Webhooks group and RECENT RUNS (tRPC
// `apps.events {id}` → the app's `automation:*` audit rows). `AppsTests`
// replays `runCases`.

public extension Apps {
    enum EventsTab {
        /// At most this many runs, within `days`.
        public static let limit = 50
        public static let days = 7
        /// automations.json `sponsorStates`: the errors that read `paused`.
        public static let pausedErrors = ["sponsorGone", "sponsorReadOnly", "sponsorNotAllowed"]

        public enum Copy {
            public static let recentRuns = "Recent runs"
            public static let noRuns = "No runs yet"
            public static let removedTrigger = "A removed trigger"
            public static let badges: [Badge: String] = [
                .ran: "Ran",
                .woke: "Woke agent",
                .failed: "Failed",
                .dropped: "Dropped",
                .paused: "Paused",
                .refused: "Refused",
            ]
            public static let errors: [String: String] = [
                "rateLimited": "30 runs a minute reached",
                "sponsorGone": "The agent's owner left the team",
                "sponsorReadOnly": "The agent's owner has read-only access",
                "sponsorNotAllowed": "Only an owner can make it run as someone else",
                "toolError": "The binding failed",
                "badSignature": "The signature did not match",
                "missingSignature": "No signature",
                "stale": "The delivery was too old",
                "replayed": "A delivery sent twice",
            ]
        }

        public enum Badge: String, Sendable, CaseIterable {
            case ran, woke, failed, dropped, paused, refused

            public var label: String { Copy.badges[self] ?? rawValue }
        }

        /// One audit row of an app's trigger (`apps.events` `runs`).
        public struct Run: Sendable, Equatable, Decodable {
            public let triggerId: String?
            /// ISO 8601.
            public let at: String
            public let status: String
            public let error: String?
            public let tool: String

            public init(triggerId: String?, at: String = "", status: String, error: String?, tool: String) {
                self.triggerId = triggerId
                self.at = at
                self.status = status
                self.error = error
                self.tool = tool
            }
        }

        public struct Row: Sendable, Equatable {
            public let title: String
            public let badge: Badge
            public let detail: String?

            public init(title: String, badge: Badge, detail: String?) {
                self.title = title
                self.badge = badge
                self.detail = detail
            }
        }
    }

    /// #EXP-1850 (`runCases`): a Recent runs row's title (the trigger's
    /// sentence without its paused suffix, else `removedTrigger`), badge and
    /// detail (`copy.errors[error]` ?? the code; nil without an error).
    static func eventRunRow(_ run: EventsTab.Run, triggers: [AgentTrigger]) -> EventsTab.Row {
        let trigger = run.triggerId.flatMap { id in triggers.first { $0.id == id } }
        let title = trigger.map { AgentSettings.triggerSummary($0.when, enabled: true) }
            ?? EventsTab.Copy.removedTrigger
        let badge: EventsTab.Badge
        if run.error == "rateLimited" {
            badge = .dropped
        } else if let error = run.error, EventsTab.pausedErrors.contains(error) {
            badge = .paused
        } else if run.status == "denied", run.error != nil {
            badge = .refused
        } else if run.status == "ok" {
            badge = run.tool.hasPrefix("agent:") ? .woke : .ran
        } else {
            badge = .failed
        }
        let detail = run.error.map { EventsTab.Copy.errors[$0] ?? $0 }
        return EventsTab.Row(title: title, badge: badge, detail: detail)
    }
}
