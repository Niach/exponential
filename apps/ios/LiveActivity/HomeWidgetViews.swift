import SwiftUI

// #EXP-1830 (home-widgets.json LOOK): the three home screen widgets' drawing.
// Tokens only: the `card` fill (the widget container), a
// header row (glyph + title sm semibold + the muted count), one-line rows
// `look.rowHeight` high with no dividers, one muted centred line when empty
// or signed out. Every tap is an `exponential://` link the app routes by the
// back stack rule (`HomeWidgets.Links`).

/// One widget's content, already folded (`HomeWidgets`).
struct HomeWidgetContent: Equatable {
    struct Row: Equatable, Identifiable {
        let id: String
        /// An asset name (the app resolved it; the widget carries the imagesets).
        let glyph: String
        let tint: Color
        let title: String
        let trailing: String
        let link: URL
    }

    let glyph: String
    let glyphTint: Color
    let title: String
    let count: Int
    let rows: [Row]
    let empty: String
    let link: URL

    static let signedOut = HomeWidgetContent(
        glyph: "", glyphTint: .clear, title: "", count: 0, rows: [], empty: HomeWidgets.Copy.signedOut,
        link: HomeWidgets.Links.app
    )

    /// A board's open issues (`board`), none picked = the first board; a
    /// picked board that is gone = the empty line, never another board.
    static func board(_ snapshot: HomeWidgets.Snapshot, boardId: String?, max: Int) -> HomeWidgetContent {
        guard snapshot.me != nil else { return .signedOut }
        let board = if let boardId {
            snapshot.boards.first { $0.id == boardId }
        } else {
            orderedBoards(snapshot).first
        }
        guard let board else {
            return HomeWidgetContent(
                glyph: "", glyphTint: .clear, title: HomeWidgets.Copy.boardGallery, count: 0, rows: [],
                empty: HomeWidgets.Copy.emptyIssues, link: HomeWidgets.Links.app
            )
        }
        let list = HomeWidgets.boardWidget(
            boardId: board.id, statuses: snapshot.statuses, issues: snapshot.issues, max: max
        )
        return HomeWidgetContent(
            glyph: board.glyph,
            glyphTint: Color(hex: board.color) ?? DesignTokens.Palette.mutedForeground,
            title: board.name,
            count: list.count,
            rows: issueRows(list.rows, snapshot),
            empty: HomeWidgets.Copy.emptyIssues,
            link: HomeWidgets.Links.board(board.id)
        )
    }

    /// The viewer's open issues in the picked teams (`my-issues`).
    static func myIssues(_ snapshot: HomeWidgets.Snapshot, teamIds: [String], max: Int) -> HomeWidgetContent {
        guard let me = snapshot.me else { return .signedOut }
        let list = HomeWidgets.myIssuesWidget(
            me: me, teamIds: teamIds, statuses: snapshot.statuses, issues: snapshot.issues, max: max
        )
        return HomeWidgetContent(
            glyph: snapshot.glyphs?.myIssues ?? "",
            glyphTint: DesignTokens.Palette.foreground,
            title: HomeWidgets.Copy.myIssues,
            count: list.count,
            rows: issueRows(list.rows, snapshot),
            empty: HomeWidgets.Copy.emptyIssues,
            link: HomeWidgets.Links.myIssues
        )
    }

    /// THE Needs you rows (`needs-you`), their ages at `now`.
    static func needsYou(_ snapshot: HomeWidgets.Snapshot, now: Date, max: Int) -> HomeWidgetContent {
        guard snapshot.me != nil else { return .signedOut }
        let list = HomeWidgets.needsYouWidget(
            rows: snapshot.needsYouRows(now: now), sealedRootIds: snapshot.sealedRootIds, max: max
        )
        let glyphs = snapshot.glyphs
        return HomeWidgetContent(
            glyph: glyphs?.needsYou ?? "",
            glyphTint: DesignTokens.Semantic.yellow,
            title: HomeWidgets.Copy.needsYou,
            count: list.count,
            rows: list.rows.map { row in
                Row(
                    id: row.messageId,
                    glyph: glyphs?.glyph(concept: row.icon) ?? "",
                    tint: DesignTokens.Semantic.yellow,
                    title: row.title,
                    trailing: row.age,
                    link: HomeWidgets.Links.needsYouRow(row)
                )
            },
            empty: HomeWidgets.Copy.emptyNeedsYou,
            link: HomeWidgets.Links.needsYou
        )
    }

    /// The boards in sidebar order: team order, then sortOrder, then name.
    static func orderedBoards(_ snapshot: HomeWidgets.Snapshot) -> [HomeWidgets.Snapshot.Board] {
        let teamRank = Dictionary(
            snapshot.teams.enumerated().map { ($0.element.id, $0.offset) }, uniquingKeysWith: { first, _ in first }
        )
        return snapshot.boards.sorted { a, b in
            let ta = teamRank[a.teamId] ?? Int.max, tb = teamRank[b.teamId] ?? Int.max
            if ta != tb { return ta < tb }
            if a.sortOrder != b.sortOrder { return a.sortOrder < b.sortOrder }
            return a.name < b.name
        }
    }

    private static func issueRows(_ rows: [HomeWidgets.IssueRow], _ snapshot: HomeWidgets.Snapshot) -> [Row] {
        let statusOf = Dictionary(snapshot.issues.map { ($0.id, $0.statusId) }, uniquingKeysWith: { first, _ in first })
        return rows.map { row in
            Row(
                id: row.id,
                glyph: statusOf[row.id].flatMap { snapshot.statusGlyphs[$0] } ?? "",
                tint: Color(hex: row.statusColor) ?? DesignTokens.Palette.mutedForeground,
                title: row.title,
                trailing: row.identifier,
                link: HomeWidgets.Links.issue(row.id)
            )
        }
    }
}

/// The widget's face: header, rows, or the one muted line.
struct HomeWidgetView: View {
    let content: HomeWidgetContent

    private typealias Look = HomeWidgets.Look

    var body: some View {
        VStack(alignment: .leading, spacing: Look.headerGap) {
            if content.title.isEmpty {
                line(content.empty)
            } else {
                header
                if content.rows.isEmpty {
                    line(content.empty)
                } else {
                    VStack(spacing: 0) {
                        ForEach(content.rows) { row in
                            Link(destination: row.link) { rowView(row) }
                        }
                    }
                    Spacer(minLength: 0)
                }
            }
        }
        .padding(Look.padding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var header: some View {
        HStack(spacing: 6) {
            glyph(content.glyph, tint: content.glyphTint)
            Text(content.title)
                .font(.system(size: DesignTokens.Typography.Size.sm, weight: .semibold))
                .foregroundStyle(DesignTokens.Palette.foreground)
                .lineLimit(1)
            Spacer(minLength: 4)
            if content.count > 0 {
                Text("\(content.count)")
                    .font(.system(size: DesignTokens.Typography.Size.sm))
                    .monospacedDigit()
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
            }
        }
    }

    private func rowView(_ row: HomeWidgetContent.Row) -> some View {
        HStack(spacing: 8) {
            glyph(row.glyph, tint: row.tint)
            Text(row.title)
                .font(.system(size: DesignTokens.Typography.Size.sm))
                .foregroundStyle(DesignTokens.Palette.foreground)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
            Text(row.trailing)
                .font(.system(size: DesignTokens.Typography.Size.xs))
                .monospacedDigit()
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .lineLimit(1)
                .layoutPriority(1)
        }
        .frame(height: Look.rowHeight)
    }

    @ViewBuilder
    private func glyph(_ asset: String, tint: Color) -> some View {
        if asset.isEmpty {
            Color.clear.frame(width: Look.glyph, height: Look.glyph)
        } else {
            Image(asset)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: Look.glyph, height: Look.glyph)
                .foregroundStyle(tint)
        }
    }

    private func line(_ text: String) -> some View {
        Text(text)
            .font(.system(size: DesignTokens.Typography.Size.sm))
            .foregroundStyle(DesignTokens.Palette.mutedForeground)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
