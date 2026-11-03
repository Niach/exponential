import Foundation

/// VAPP-234: the template gallery's list and copy, the iOS mirror of
/// `packages/domain-contract/fixtures/app-templates.json` (`AppTemplatesTests`
/// locks them to it). Picking one is the SERVER's (`apps.importTemplate`:
/// the pinned signature, then the import).
public enum AppTemplates {
    public struct Template: Equatable, Sendable, Identifiable {
        public let id: String
        public let name: String
        public let icon: String
        public let color: String
        public let description: String

        public init(id: String, name: String, icon: String, color: String, description: String) {
            self.id = id
            self.name = name
            self.icon = icon
            self.color = color
            self.description = description
        }
    }

    public enum Copy {
        public static let title = "Start from a template"
        public static let fromTemplate = "Start from a template"
        public static let includeRecords = "Include sample records"
        public static let note = "From a template"
    }

    /// Web `data-testid`s ×4.
    public enum TestIds {
        public static let row = "app-template"
        public static let records = "app-template-records"
        public static let fromTemplate = "app-from-template"
    }

    /// The gallery, in order.
    public static let templates: [Template] = [
        Template(id: "time-tracker", name: "Time tracker", icon: "clock", color: "#10b981", description: "Hours per day and week against a monthly target"),
        Template(id: "crm", name: "CRM", icon: "briefcase", color: "#6366f1", description: "Companies, contacts and the deal pipeline"),
        Template(id: "standup", name: "Standup", icon: "users", color: "#f59e0b", description: "The async daily standup"),
    ]
}
