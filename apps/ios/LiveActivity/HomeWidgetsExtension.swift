import AppIntents
import SwiftUI
import WidgetKit

// #EXP-1830 (home-widgets.json): the three home screen widgets, `board` (a
// board picked in the edit sheet), `my-issues` (teams picked, several; none =
// every team) and `needs-you`. They read the snapshot the app writes into the
// app group (`HomeWidgets.readSnapshot`) and fold it with `HomeWidgets`; the
// app reloads the timelines on every change and after sign-in/out.

// MARK: - Configuration entities (the snapshot's boards and teams)

struct WidgetBoardEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Board"
    static let defaultQuery = WidgetBoardQuery()

    let id: String
    let name: String

    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }
}

struct WidgetBoardQuery: EntityQuery {
    private func all() -> [WidgetBoardEntity] {
        guard let snapshot = HomeWidgets.readSnapshot() else { return [] }
        return HomeWidgetContent.orderedBoards(snapshot).map { WidgetBoardEntity(id: $0.id, name: $0.name) }
    }

    func entities(for identifiers: [String]) async throws -> [WidgetBoardEntity] {
        all().filter { identifiers.contains($0.id) }
    }

    func suggestedEntities() async throws -> [WidgetBoardEntity] { all() }

    func defaultResult() async -> WidgetBoardEntity? { all().first }
}

struct WidgetTeamEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Team"
    static let defaultQuery = WidgetTeamQuery()

    let id: String
    let name: String

    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }
}

struct WidgetTeamQuery: EntityQuery {
    private func all() -> [WidgetTeamEntity] {
        (HomeWidgets.readSnapshot()?.teams ?? []).map { WidgetTeamEntity(id: $0.id, name: $0.name) }
    }

    func entities(for identifiers: [String]) async throws -> [WidgetTeamEntity] {
        all().filter { identifiers.contains($0.id) }
    }

    func suggestedEntities() async throws -> [WidgetTeamEntity] { all() }
}

struct BoardWidgetIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Board"

    // The fixture's `copy.pickBoard` (App Intents metadata takes literals only).
    @Parameter(title: "Board")
    var board: WidgetBoardEntity?

    init() {}
}

struct MyIssuesWidgetIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "My issues"

    // The fixture's `copy.pickTeams`.
    @Parameter(title: "Teams")
    var teams: [WidgetTeamEntity]?

    init() {}
}

// MARK: - Timelines

struct HomeWidgetEntry: TimelineEntry {
    let date: Date
    let content: HomeWidgetContent
}

private func capacity(_ family: WidgetFamily) -> Int {
    switch family {
    case .systemSmall: HomeWidgets.Capacity.small
    case .systemLarge, .systemExtraLarge: HomeWidgets.Capacity.large
    default: HomeWidgets.Capacity.medium
    }
}

/// The app reloads on every change; the clock alone moves nothing but the
/// needs-you ages, so the rest asks again in an hour.
private let refresh: TimeInterval = 3600

struct BoardWidgetProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> HomeWidgetEntry {
        HomeWidgetEntry(date: Date(), content: .signedOut)
    }

    func snapshot(for configuration: BoardWidgetIntent, in context: Context) async -> HomeWidgetEntry {
        entry(configuration, context)
    }

    func timeline(for configuration: BoardWidgetIntent, in context: Context) async -> Timeline<HomeWidgetEntry> {
        Timeline(entries: [entry(configuration, context)], policy: .after(Date().addingTimeInterval(refresh)))
    }

    private func entry(_ configuration: BoardWidgetIntent, _ context: Context) -> HomeWidgetEntry {
        let snapshot = HomeWidgets.readSnapshot() ?? .signedOut
        return HomeWidgetEntry(
            date: Date(),
            content: .board(snapshot, boardId: configuration.board?.id, max: capacity(context.family))
        )
    }
}

struct MyIssuesWidgetProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> HomeWidgetEntry {
        HomeWidgetEntry(date: Date(), content: .signedOut)
    }

    func snapshot(for configuration: MyIssuesWidgetIntent, in context: Context) async -> HomeWidgetEntry {
        entry(configuration, context)
    }

    func timeline(for configuration: MyIssuesWidgetIntent, in context: Context) async -> Timeline<HomeWidgetEntry> {
        Timeline(entries: [entry(configuration, context)], policy: .after(Date().addingTimeInterval(refresh)))
    }

    private func entry(_ configuration: MyIssuesWidgetIntent, _ context: Context) -> HomeWidgetEntry {
        let snapshot = HomeWidgets.readSnapshot() ?? .signedOut
        return HomeWidgetEntry(
            date: Date(),
            content: .myIssues(
                snapshot, teamIds: (configuration.teams ?? []).map(\.id), max: capacity(context.family)
            )
        )
    }
}

struct NeedsYouWidgetProvider: TimelineProvider {
    /// One entry a minute for this long, so the ages move with the clock.
    private static let minutes = 60

    func placeholder(in context: Context) -> HomeWidgetEntry {
        HomeWidgetEntry(date: Date(), content: .signedOut)
    }

    func getSnapshot(in context: Context, completion: @escaping (HomeWidgetEntry) -> Void) {
        completion(entry(HomeWidgets.readSnapshot() ?? .signedOut, at: Date(), context))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<HomeWidgetEntry>) -> Void) {
        let now = Date()
        // One read: every minute's entry folds the same snapshot.
        let snapshot = HomeWidgets.readSnapshot() ?? .signedOut
        let entries = (0..<Self.minutes).map {
            entry(snapshot, at: now.addingTimeInterval(Double($0) * 60), context)
        }
        completion(Timeline(entries: entries, policy: .atEnd))
    }

    private func entry(_ snapshot: HomeWidgets.Snapshot, at date: Date, _ context: Context) -> HomeWidgetEntry {
        HomeWidgetEntry(date: date, content: .needsYou(snapshot, now: date, max: capacity(context.family)))
    }
}

// MARK: - The widgets

/// The platform's widget container filled with the `card` token.
private struct HomeWidgetEntryView: View {
    let entry: HomeWidgetEntry

    var body: some View {
        HomeWidgetView(content: entry.content)
            .widgetURL(entry.content.link)
            .containerBackground(DesignTokens.Palette.card, for: .widget)
    }
}

private let families: [WidgetFamily] = [.systemSmall, .systemMedium, .systemLarge]

struct BoardHomeWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(
            kind: HomeWidgets.Kind.board.rawValue, intent: BoardWidgetIntent.self, provider: BoardWidgetProvider()
        ) { entry in
            HomeWidgetEntryView(entry: entry)
        }
        .configurationDisplayName(HomeWidgets.Copy.boardGallery)
        .description(HomeWidgets.Copy.boardGalleryDescription)
        .supportedFamilies(families)
        .contentMarginsDisabled()
    }
}

struct MyIssuesHomeWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(
            kind: HomeWidgets.Kind.myIssues.rawValue, intent: MyIssuesWidgetIntent.self,
            provider: MyIssuesWidgetProvider()
        ) { entry in
            HomeWidgetEntryView(entry: entry)
        }
        .configurationDisplayName(HomeWidgets.Copy.myIssues)
        .description(HomeWidgets.Copy.myIssuesGalleryDescription)
        .supportedFamilies(families)
        .contentMarginsDisabled()
    }
}

struct NeedsYouHomeWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: HomeWidgets.Kind.needsYou.rawValue, provider: NeedsYouWidgetProvider()) { entry in
            HomeWidgetEntryView(entry: entry)
        }
        .configurationDisplayName(HomeWidgets.Copy.needsYou)
        .description(HomeWidgets.Copy.needsYouGalleryDescription)
        .supportedFamilies(families)
        .contentMarginsDisabled()
    }
}
