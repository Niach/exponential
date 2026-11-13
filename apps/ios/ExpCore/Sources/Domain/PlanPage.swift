import Foundation

/// #EXP-1493 → #EXP-1662 THE PLAN TAB ×4 (`packages/domain-contract/fixtures/plan-page.json`,
/// TS reference `src/plan-page.ts`): the expanded form of THE plan card, a
/// tab of its chat. A plan's markdown split into blocks (title, prose, h2
/// sections, picture groups), its outline, its counts line, which plan the
/// tab shows, its row's label and where a session's waiting plan opens.
/// `PlanPageTests` replays every `*Cases` array.
public enum PlanPage {
    // MARK: - Fixture constants

    public enum Copy {
        public static let title = "Plan"
        public static let openPlan = "Open plan"
        public static let openFull = "Open full plan"
        public static let openChat = "Open chat"
        public static let outline = "Outline"
        public static let questions = "Questions"
        public static let needsYou = "Needs you"
        public static let rowPrefix = "Plan: "
        /// singular, plural.
        public static let sections = ["section", "sections"]
        public static let pictures = ["picture", "pictures"]
        public static let questionsCount = ["question", "questions"]
    }

    public enum Layout {
        public static let outlineWidth: Double = 240
        /// #FEED-191: the wide questions rail (phones keep the floating panel).
        public static let railWidth: Double = 360
        public static let columnMaxWidth: Double = 720
        public static let columnPaddingWide: Double = 28
        public static let columnPaddingPhone: Double = 18
        public static let blockGap: Double = 14
        public static let outlineRowHeight: Double = 32
        public static let outlineActiveBar: Double = 2
        public static let titleSize: Double = 28
        public static let rowHeight: Double = 44
        public static let tabMaxWidth: Double = 260
        /// #EXP-1786: the document's readable floor (`planPaneLayout`).
        public static let documentMinWidth: Double = 360
    }

    /// #EXP-1662: the questions float over the document's foot.
    public enum Panel {
        public static let maxWidth: Double = 720
        public static let gap: Double = 16
        public static let padding: Double = 16
        public static let radius: Double = 16
        public static let maxHeightFraction: Double = 0.4
    }

    public enum Mosaic {
        public static let gap: Double = 10
        public static let largeFraction: Double = 0.6
        public static let aspect: Double = 1.6
        public static let radius: Double = 10
    }

    public enum TestIds {
        public static let page = "plan-page"
        public static let outline = "plan-outline"
        public static let outlineRow = "plan-outline-row"
        public static let document = "plan-document"
        public static let rail = "plan-rail"
        public static let mosaic = "plan-mosaic"
        public static let carousel = "plan-carousel"
        public static let openPlan = "open-plan"
        public static let outlineToggle = "plan-outline-toggle"
        public static let tab = "chat-tab-plan"
    }

    public static let route = "/t/$teamSlug/sessions/$sessionId/plan/$messageId"

    /// THE styleguide specimen ×4 (`specimen`): the chat's name and the
    /// active outline row; its card is read from the fixture by the capture.
    public enum Specimen {
        public static let chatName = "Editor-first app creation, eApps MCP"
        public static let activeSection = "part-a-editor-first-creation"
    }

    // MARK: - Model

    public struct Picture: Equatable, Sendable, Hashable {
        public let alt: String
        public let src: String

        public init(alt: String, src: String) {
            self.alt = alt
            self.src = src
        }
    }

    public enum Block: Equatable, Sendable {
        /// #EXP-1662: a leading `# ` line.
        case title(String)
        case markdown(String)
        case section(id: String, title: String)
        case pictures([Picture])
    }

    public struct Section: Equatable, Sendable, Identifiable {
        public let id: String
        public let title: String
        /// Pictures inside the section (grouped or single).
        public var pictures: Int

        public init(id: String, title: String, pictures: Int) {
            self.id = id
            self.title = title
            self.pictures = pictures
        }
    }

    public struct Counts: Equatable, Sendable {
        public let sections: Int
        public let pictures: Int
        public let questions: Int

        public init(sections: Int, pictures: Int, questions: Int) {
            self.sections = sections
            self.pictures = pictures
            self.questions = questions
        }
    }

    nonisolated(unsafe) private static let pictureLine = #/^\s*!\[([^\]]*)\]\(([^)\s]+)\)\s*$/#
    nonisolated(unsafe) private static let pictureAny = #/!\[[^\]]*\]\(([^)\s]+)\)/#
    nonisolated(unsafe) private static let h1 = #/^#\s+(.+?)\s*#*\s*$/#
    nonisolated(unsafe) private static let h2 = #/^##\s+(.+?)\s*#*\s*$/#
    nonisolated(unsafe) private static let fence = #/^\s*(```|~~~)/#
    nonisolated(unsafe) private static let link = #/\[([^\]]*)\]\([^)]*\)/#
    nonisolated(unsafe) private static let planPrefix = #/^plan:\s*/#.ignoresCase()
    private static let pending = "upload:"

    private static func isFence(_ line: String) -> Bool { line.prefixMatch(of: fence) != nil }

    /// A heading's anchor: lowercase, runs of anything but a-z0-9 = `-`.
    public static func sectionSlug(_ title: String) -> String {
        var out = ""
        var dash = false
        for scalar in title.lowercased().unicodeScalars {
            let ok = ("a"..."z").contains(scalar) || ("0"..."9").contains(scalar)
            if ok {
                if dash && !out.isEmpty { out.append("-") }
                dash = false
                out.unicodeScalars.append(scalar)
            } else {
                dash = true
            }
        }
        return out.isEmpty ? "section" : out
    }

    private static func plainTitle(_ raw: String) -> String {
        raw.replacing(link) { String($0.1) }
            .replacingOccurrences(of: "**", with: "")
            .replacingOccurrences(of: "__", with: "")
            .replacingOccurrences(of: "`", with: "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// #EXP-1662 / #FEED-191: the plan a chat's tab shows (`tabCases`): the
    /// newest waiting card, else `kept` while it is an approved or rejected
    /// plan of the chat (the plan the tab showed during THIS visit of the
    /// chat; the host clears it when the person leaves), else none: a decided
    /// plan leaves the strip. `plans` = oldest first.
    public static func planTabTarget(_ plans: [(id: String, decision: String?)], kept: String? = nil) -> String? {
        if let waiting = plans.last(where: { $0.decision == nil }) { return waiting.id }
        guard let kept, let held = plans.first(where: { $0.id == kept }) else { return nil }
        return held.decision == "approved" || held.decision == "rejected" ? held.id : nil
    }

    /// #FEED-191: the plan tab's lead glyph (`tabGlyphCases`).
    public struct TabGlyph: Equatable, Sendable {
        /// `coding-plan` | `ui-check`.
        public let icon: String
        /// `text` = the chip's own text colour, `approved` = emerald (the
        /// plan row's approved check).
        public let tone: String

        public init(icon: String, tone: String) {
            self.icon = icon
            self.tone = tone
        }
    }

    /// An approved plan = THE check in emerald (as its row), anything else =
    /// the plan glyph.
    public static func planTabGlyph(_ decision: String?) -> TabGlyph {
        decision == "approved" ? TabGlyph(icon: "ui-check", tone: "approved") : TabGlyph(icon: "coding-plan", tone: "text")
    }

    /// #EXP-1786: which wide columns the plan body's OWN width holds
    /// (`paneCases`). `outline` false = no outline column; `rail` false = the
    /// questions float (`Panel`).
    public struct PaneLayout: Equatable, Sendable {
        public let outline: Bool
        public let rail: Bool

        public init(outline: Bool, rail: Bool) {
            self.outline = outline
            self.rail = rail
        }
    }

    /// The three columns while the document keeps `documentMinWidth` beside
    /// both, else the document + the rail, else the document alone.
    public static func planPaneLayout(_ width: Double) -> PaneLayout {
        PaneLayout(
            outline: width >= Layout.outlineWidth + Layout.railWidth + Layout.documentMinWidth,
            rail: width >= Layout.railWidth + Layout.documentMinWidth
        )
    }

    /// #EXP-1662: `Plan: title`, the row's and the wide tab's label
    /// (`rowLabelCases`).
    public static func planRowLabel(_ plan: String) -> String {
        let title = AskDock.title(plan).replacing(planPrefix, with: "")
        return title.isEmpty ? Copy.title : Copy.rowPrefix + title
    }

    /// THE plan page's blocks (`blockCases`): a leading `# ` line = the `title`
    /// (#EXP-1662), every `## ` line outside a code fence = a `section` (anchor deduped `-2`, `-3`…), a run of two or more
    /// picture-only lines (blank lines between allowed) = ONE `pictures`
    /// block; a lone picture stays in the prose; a pending `upload:` picture
    /// is dropped. Prose blocks are trimmed of blank edge lines; empty = dropped.
    public static func blocks(_ markdown: String) -> [Block] {
        var blocks: [Block] = []
        var seen: [String: Int] = [:]
        var prose: [String] = []
        var run: [Picture] = []
        var runLines: [String] = []
        var fenced = false
        var lines = markdown.components(separatedBy: "\n")

        func blankLine(_ line: String) -> Bool { line.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        if let first = lines.firstIndex(where: { !blankLine($0) }), let heading = lines[first].wholeMatch(of: h1) {
            blocks.append(.title(plainTitle(String(heading.1))))
            lines.remove(at: first)
        }
        func flushProse() {
            while let first = prose.first, blankLine(first) { prose.removeFirst() }
            while let last = prose.last, blankLine(last) { prose.removeLast() }
            if !prose.isEmpty { blocks.append(.markdown(prose.joined(separator: "\n"))) }
            prose = []
        }
        func flushRun() {
            if run.count >= 2 {
                flushProse()
                blocks.append(.pictures(run))
            } else {
                prose.append(contentsOf: runLines)
            }
            run = []
            runLines = []
        }

        for line in lines {
            if isFence(line) {
                flushRun()
                fenced.toggle()
                prose.append(line)
                continue
            }
            if fenced {
                prose.append(line)
                continue
            }
            if let picture = line.wholeMatch(of: pictureLine) {
                let src = String(picture.2)
                if src.hasPrefix(pending) { continue }
                run.append(Picture(alt: String(picture.1), src: src))
                runLines.append(line)
                continue
            }
            if blankLine(line) && !run.isEmpty {
                runLines.append(line)
                continue
            }
            flushRun()
            if let heading = line.wholeMatch(of: h2) {
                flushProse()
                let title = plainTitle(String(heading.1))
                let base = sectionSlug(title)
                let n = (seen[base] ?? 0) + 1
                seen[base] = n
                blocks.append(.section(id: n == 1 ? base : "\(base)-\(n)", title: title))
                continue
            }
            prose.append(line.replacing(pictureAny) { match in
                match.1.hasPrefix(pending) ? "" : String(match.0)
            })
        }
        flushRun()
        flushProse()
        return blocks
    }

    /// The pictures a block shows (a fenced code block shows none).
    private static func picturesIn(_ block: Block) -> Int {
        switch block {
        case let .pictures(pictures): return pictures.count
        case .title, .section: return 0
        case let .markdown(text):
            var fenced = false
            var count = 0
            for line in text.components(separatedBy: "\n") {
                if isFence(line) {
                    fenced.toggle()
                } else if !fenced {
                    count += line.matches(of: pictureAny).filter { !$0.1.hasPrefix(pending) }.count
                }
            }
            return count
        }
    }

    /// THE outline (`outlineCases`): one row per `section` block, in order,
    /// with the pictures between it and the next section.
    public static func outline(_ markdown: String) -> [Section] {
        var out: [Section] = []
        for block in blocks(markdown) {
            if case let .section(id, title) = block {
                out.append(Section(id: id, title: title, pictures: 0))
            } else if !out.isEmpty {
                out[out.count - 1].pictures += picturesIn(block)
            }
        }
        return out
    }

    /// Every picture of a plan in document order (THE lightbox's list).
    public static func pictures(_ markdown: String) -> [Picture] {
        var out: [Picture] = []
        for block in blocks(markdown) {
            switch block {
            case let .pictures(pictures): out.append(contentsOf: pictures)
            case .title, .section: break
            case let .markdown(text):
                var fenced = false
                for line in text.components(separatedBy: "\n") {
                    if isFence(line) {
                        fenced.toggle()
                    } else if !fenced {
                        for match in line.matches(of: #/!\[([^\]]*)\]\(([^)\s]+)\)/#) where !match.2.hasPrefix(pending) {
                            out.append(Picture(alt: String(match.1), src: String(match.2)))
                        }
                    }
                }
            }
        }
        return out
    }

    /// A plan card's counts (`countCases`): its h2 sections, its RESOLVED
    /// pictures (attachmentId set), its questions.
    public static func counts(_ card: TreeMessageCard?) -> Counts {
        Counts(
            sections: outline(card?.plan ?? "").count,
            pictures: card?.pictures.filter { $0.attachmentId != nil }.count ?? 0,
            questions: card?.questions.count ?? 0
        )
    }

    /// "5 sections · 10 pictures · 7 questions": zero parts left out, the
    /// singular for one; nothing = "".
    public static func countsLine(_ counts: Counts) -> String {
        [(counts.sections, Copy.sections), (counts.pictures, Copy.pictures), (counts.questions, Copy.questionsCount)]
            .filter { $0.0 > 0 }
            .map { "\($0.0) \($0.1[$0.0 == 1 ? 0 : 1])" }
            .joined(separator: " · ")
    }

    public enum TileSize: String, Equatable, Sendable {
        case large, half, small
    }

    public struct MosaicTile: Equatable, Sendable {
        public let index: Int
        public let size: TileSize
        /// "+N" on the last small tile when the group has more than it shows.
        public let more: Int

        public init(index: Int, size: TileSize, more: Int) {
            self.index = index
            self.size = size
            self.more = more
        }
    }

    /// THE wide picture group (eUI `Mosaic`, `mosaicCases`). Phones draw a
    /// carousel instead; kept for the contract.
    public static func mosaicTiles(_ count: Int) -> [MosaicTile] {
        if count <= 0 { return [] }
        if count == 1 { return [MosaicTile(index: 0, size: .large, more: 0)] }
        if count == 2 { return [0, 1].map { MosaicTile(index: $0, size: .half, more: 0) } }
        return [
            MosaicTile(index: 0, size: .large, more: 0),
            MosaicTile(index: 1, size: .small, more: 0),
            MosaicTile(index: 2, size: .small, more: count - 3),
        ]
    }

    public struct Message: Equatable, Sendable {
        public let id: String
        public let sessionId: String
        public let authorSessionId: String?
        /// Epoch ms.
        public let createdAt: Double
        public let card: TreeMessageCard?

        public init(id: String, sessionId: String, authorSessionId: String?, createdAt: Double, card: TreeMessageCard?) {
            self.id = id
            self.sessionId = sessionId
            self.authorSessionId = authorSessionId
            self.createdAt = createdAt
            self.card = card
        }
    }

    /// Where a session's waiting plan opens (`targetCases`): the NEWEST plan
    /// card the session posted itself (no engine `questionId`, no decision);
    /// nil = none (the host opens the session).
    public static func planPageFor(_ sessionId: String, _ messages: [Message]) -> String? {
        var best: Message?
        for message in messages {
            guard message.sessionId == sessionId, message.authorSessionId == sessionId,
                  let card = message.card, card.kind == "plan", card.decision == nil, card.questionId == nil
            else { continue }
            if let kept = best, !(message.createdAt > kept.createdAt
                || (message.createdAt == kept.createdAt && message.id > kept.id)) { continue }
            best = message
        }
        return best?.id
    }

    /// The web deep link; natives open the chat's tab on the same plan.
    public static func path(teamSlug: String, sessionId: String, messageId: String) -> String {
        route
            .replacingOccurrences(of: "$teamSlug", with: teamSlug)
            .replacingOccurrences(of: "$sessionId", with: sessionId)
            .replacingOccurrences(of: "$messageId", with: messageId)
    }
}
