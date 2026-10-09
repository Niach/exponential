import CoreGraphics
import Foundation

// EXP-879: a coding run's published RESULTS — the screenshots the agent filed
// with `exponential_sessions_guide` (EXP-1251, alias
// `exponential_sessions_results`) while it worked, read off the synced
// `coding_sessions.results` jsonb (the entity keeps it as the raw TEXT, like
// `blocked`).
//
// The blob is a FLAT, ORDERED list: `{ topic, label, attachmentId, width,
// height }`. Grouping is derived, never stored, so an agent that publishes
// `web` then `ios` under one topic and later a second topic keeps the order it
// chose. `(topic, label)` is the upsert key (the server replaces in place), so
// a client only ever renders what it reads.
//
// These are the PURE rules every client mirrors byte for byte: web
// `lib/session-results.ts` (the spec), desktop `crates/ui/src/session_results.rs`,
// Android `domain/SessionResults.kt` — same names, same order, same test names.

/// The cap the server enforces on a single run's list.
public let maxSessionResults = 60

/// Every tile renders at ONE height; the probed aspect gives its width, so a
/// row of an iOS, an Android and a web shot reads as one strip.
public let sessionResultTileHeight: CGFloat = 320

/// EXP-1128: a picture whose probed width/height is UNDER this is TALL (a
/// full-page capture; a phone shot at ~0.46 never is). A tall picture takes
/// the 4:3 frame top-cropped with a Tall badge instead of rendering as a
/// sliver, and opens fit-to-width in a vertical scroll. Strict: exactly 1:3
/// is not tall. Fixture `session-results.json` `tiles` (×4).
public let sessionResultTallAspect: CGFloat = 1.0 / 3.0

/// EXP-1172: an `exponential_sessions_show` picture's tile in the run
/// transcript, one base height lower than a Results tile so a shot sits in the
/// conversation without swallowing it. Fixture `session-inline.json`.
public let sessionInlineTileHeight: CGFloat = 240

/// EXP-1172: the collapsed band a Results group folds its inline pictures
/// under (`Earlier · 3`).
public let sessionResultsEarlierLabel = "Earlier"

public struct SessionResultEntry: Equatable, Sendable {
    public let topic: String
    public let label: String
    public let attachmentId: String
    /// Probed at upload; nil when the image could not be measured.
    public let width: Int?
    public let height: Int?
    /// EXP-1172: filed by `exponential_sessions_show` while the run worked
    /// (true only for a JSON true).
    public let inline: Bool
    /// EXP-1172: the show call's `text`, trimmed; nil when blank.
    public let caption: String?

    public init(
        topic: String,
        label: String,
        attachmentId: String,
        width: Int? = nil,
        height: Int? = nil,
        inline: Bool = false,
        caption: String? = nil
    ) {
        self.topic = topic
        self.label = label
        self.attachmentId = attachmentId
        self.width = width
        self.height = height
        self.inline = inline
        self.caption = caption
    }

    /// EXP-1172: the line under a transcript tile — the show call's caption,
    /// else the picture's label.
    public var tileCaption: String { caption ?? label }

    /// The stored, RELATIVE attachment URL — the same member-gated path a
    /// comment's image carries, so the shared loader (and its process cache)
    /// fetch it exactly like any other attachment.
    public var url: String { "/api/attachments/\(attachmentId)" }
}

/// A trimmed non-empty string, or nil.
private func resultText(_ value: Any?) -> String? {
    guard let string = value as? String else { return nil }
    let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
}

/// A probed pixel dimension, or nil: a zero, a negative, a fraction, a NaN and
/// anything that is not a number all mean "unknown" and fall back to 4:3.
private func resultDimension(_ value: Any?) -> Int? {
    guard let number = value as? NSNumber,
          CFGetTypeID(number) != CFBooleanGetTypeID()
    else { return nil }
    let double = number.doubleValue
    guard double.isFinite, double > 0, double == double.rounded(),
          double <= Double(Int.max)
    else { return nil }
    return Int(double)
}

/// The blob's object rows: nil, blank, unparseable and non-array all read as
/// none, and a non-object row is dropped.
private func resultRecords(_ raw: String?) -> [[String: Any]] {
    guard let raw else { return [] }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty,
          let data = trimmed.data(using: .utf8),
          let object = try? JSONSerialization.jsonObject(with: data),
          let rows = object as? [Any]
    else { return [] }
    return rows.compactMap { $0 as? [String: Any] }
}

/// A PICTURE entry needs topic + label + attachmentId (a text field on it is
/// ignored: it is still a picture).
private func resultPicture(_ record: [String: Any]) -> SessionResultEntry? {
    guard let topic = resultText(record["topic"]),
          let label = resultText(record["label"]),
          let attachmentId = resultText(record["attachmentId"])
    else { return nil }
    return SessionResultEntry(
        topic: topic,
        label: label,
        attachmentId: attachmentId,
        width: resultDimension(record["width"]),
        height: resultDimension(record["height"]),
        // EXP-1172: a JSON true only — `"true"` and `1` read as false.
        inline: (record["inline"] as? NSNumber).map {
            CFGetTypeID($0) == CFBooleanGetTypeID() && $0.boolValue
        } ?? false,
        caption: resultText(record["caption"])
    )
}

/// Tolerant reader for the jsonb column as the entity stores it: raw JSON
/// text. nil, blank and unparseable all read as "no results", and a malformed
/// entry is DROPPED rather than rendered as a broken tile. The list is capped
/// exactly like the writer caps it. Pictures only (EXP-933 text entries are
/// read by `parseSessionResultGroups`).
public func parseSessionResults(_ raw: String?) -> [SessionResultEntry] {
    var entries: [SessionResultEntry] = []
    for record in resultRecords(raw) {
        guard let entry = resultPicture(record) else { continue }
        entries.append(entry)
        if entries.count >= maxSessionResults { break }
    }
    return entries
}

public struct SessionResultGroup: Equatable, Sendable, Identifiable {
    public let topic: String
    /// EXP-933: the topic's GFM report text (rendered ABOVE its pictures), nil
    /// without one.
    public let text: String?
    public let entries: [SessionResultEntry]
    /// EXP-1172: the topic's INLINE pictures, folded under the `Earlier` band
    /// (publish order); empty when the topic has a single picture.
    public let earlier: [SessionResultEntry]
    /// EXP-1154: the repo-relative paths the topic touched, off the SAME
    /// entry its text came from; empty when none (or a picture-only topic).
    public let files: [String]
    /// EXP-1251: the PR the topic belongs to (its text entry's `prUrl`); nil =
    /// every PR of the run.
    public let prUrl: String?

    public init(
        topic: String,
        text: String? = nil,
        entries: [SessionResultEntry],
        earlier: [SessionResultEntry] = [],
        files: [String] = [],
        prUrl: String? = nil
    ) {
        self.topic = topic
        self.text = text
        self.entries = entries
        self.earlier = earlier
        self.files = files
        self.prUrl = prUrl
    }

    public var id: String { topic }
}

/// EXP-1172: a topic with more than one picture moves its inline ones into
/// `earlier`, so the final report leads; a topic's only picture stays.
private func foldInline(_ group: SessionResultGroup) -> SessionResultGroup {
    guard group.entries.count >= 2 else { return group }
    return SessionResultGroup(
        topic: group.topic,
        text: group.text,
        entries: group.entries.filter { !$0.inline },
        earlier: group.entries.filter(\.inline),
        files: group.files,
        prUrl: group.prUrl
    )
}

/// EXP-933: the Results face as a REPORT — pictures AND each topic's text
/// (`{topic, label: null, attachmentId: null, text}`), grouped in FIRST-SEEN
/// topic order whichever kind opened the topic. A topic's text is its FIRST
/// non-blank text entry, trimmed; pictures keep the 60 cap. Fixture:
/// `packages/domain-contract/fixtures/session-results.json` (×4).
public func parseSessionResultGroups(_ raw: String?) -> [SessionResultGroup] {
    sessionResultGroups(resultRecords(raw))
}

/// The same reader over records already parsed (what `sessionResultsForPr`
/// answers).
public func parseSessionResultGroups(records: [[String: Any]]) -> [SessionResultGroup] {
    sessionResultGroups(records)
}

private func sessionResultGroups(_ records: [[String: Any]]) -> [SessionResultGroup] {
    var topics: [String] = []
    var texts: [String: String] = [:]
    var filesByTopic: [String: [String]] = [:]
    var prUrlByTopic: [String: String] = [:]
    var byTopic: [String: [SessionResultEntry]] = [:]
    func open(_ topic: String) {
        if byTopic[topic] == nil {
            topics.append(topic)
            byTopic[topic] = []
        }
    }
    var pictures = 0
    for record in records {
        if let entry = resultPicture(record) {
            if pictures >= maxSessionResults { continue }
            pictures += 1
            open(entry.topic)
            byTopic[entry.topic]?.append(entry)
            continue
        }
        guard let topic = resultText(record["topic"]),
              let body = resultText(record["text"])
        else { continue }
        open(topic)
        if texts[topic] == nil {
            texts[topic] = body
            filesByTopic[topic] = resultFiles(record["files"])
            if let prUrl = resultText(record["prUrl"]) { prUrlByTopic[topic] = prUrl }
        }
    }
    return topics.map {
        foldInline(SessionResultGroup(
            topic: $0,
            text: texts[$0],
            entries: byTopic[$0] ?? [],
            files: filesByTopic[$0] ?? [],
            prUrl: prUrlByTopic[$0]
        ))
    }
}

/// EXP-1154: the cap on one topic's `files` (fixture `files.maxFiles`).
public let maxSessionResultFiles = 40

/// EXP-1154: a text entry's `files` — strings only, trimmed, blanks and
/// duplicates dropped (first position kept), capped; a non-array is none.
private func resultFiles(_ value: Any?) -> [String] {
    guard let rows = value as? [Any] else { return [] }
    var seen = Set<String>()
    var files: [String] = []
    for row in rows {
        // A JSON number bridges to NSNumber, never String.
        guard let path = resultText(row), !seen.contains(path) else { continue }
        seen.insert(path)
        files.append(path)
        if files.count >= maxSessionResultFiles { break }
    }
    return files
}

// MARK: - EXP-1154: the Results face as the GUIDE

/// The topic the Guide draws as its unnumbered lead paragraph.
public let sessionResultsSummaryTopic = "Summary"

/// The trimmed topic equals `Summary`, case-insensitively.
public func isSummaryTopic(_ topic: String) -> Bool {
    topic.trimmingCharacters(in: .whitespacesAndNewlines)
        .caseInsensitiveCompare(sessionResultsSummaryTopic) == .orderedSame
}

public struct SessionResultsGuide: Equatable, Sendable {
    public struct Section: Equatable, Sendable, Identifiable {
        public let group: SessionResultGroup
        /// 1-based; the lead never counts.
        public let index: Int
        public let total: Int

        public var id: String { group.topic }
    }

    /// The FIRST Summary group wherever it sits, nil without one.
    public let lead: SessionResultGroup?
    public let sections: [Section]
}

/// The lead (the first Summary group) and every other group numbered in
/// order. Fixture `session-results.json` `guide.sections` (×4).
public func sessionResultsGuide(_ groups: [SessionResultGroup]) -> SessionResultsGuide {
    let leadIndex = groups.firstIndex { isSummaryTopic($0.topic) }
    let rest = groups.enumerated().filter { $0.offset != leadIndex }.map(\.element)
    return SessionResultsGuide(
        lead: leadIndex.map { groups[$0] },
        sections: rest.enumerated().map { offset, group in
            SessionResultsGuide.Section(group: group, index: offset + 1, total: rest.count)
        }
    )
}

/// A section's caption, two-digit zero-padded: `01 / 04`.
public func guideSectionCaption(_ index: Int, _ total: Int) -> String {
    String(format: "%02d / %02d", index, total)
}

// MARK: - EXP-1251: the Guide's COVERAGE

// Each section's ONE `Changes` row counts the diff files its topic names (a
// path matches a diff file's `path` OR its rename source `previousPath`),
// every diff file no topic names lands in a trailing automatic section
// (`Other changes`; with no report at all, ONE `Changes` section holds the
// whole diff) and `complete` is the `Show complete diff` row. Fixture
// `session-results.json` `coverage` (×4).

/// The automatic section's title when a report exists.
public let guideOtherChangesTopic = DomainContract.diffUiGuideOtherChanges
/// The automatic section's title with no report, and every section's row label.
public let guideChangesTopic = DomainContract.diffUiGuideChangesRow

public struct GuideChangeSet: Equatable, Sendable {
    public let files: [Diff.File]
    public let additions: Int
    public let deletions: Int
    public var fileCount: Int { files.count }

    public init(files: [Diff.File]) {
        self.files = files
        additions = files.reduce(0) { $0 + $1.additions }
        deletions = files.reduce(0) { $0 + $1.deletions }
    }
}

public struct GuideCoveredGroup: Equatable, Sendable {
    public let group: SessionResultGroup
    /// nil while no diff is loaded: the row is not drawn.
    public let changes: GuideChangeSet?
    /// Listed paths the loaded diff does not have (empty without a diff).
    public let missing: [String]
}

public struct GuideCoverageSection: Equatable, Sendable, Identifiable {
    public let group: SessionResultGroup
    public let changes: GuideChangeSet?
    public let missing: [String]
    /// 1-based; the lead never counts, the automatic section neither.
    public let index: Int
    public let total: Int

    public var id: String { group.topic }
}

public struct GuideCoverage: Equatable, Sendable {
    public struct Other: Equatable, Sendable {
        public let topic: String
        public let changes: GuideChangeSet
    }

    public let lead: GuideCoveredGroup?
    public let sections: [GuideCoverageSection]
    /// The trailing automatic section, unnumbered: nil when every file is
    /// claimed, the diff is empty or not loaded.
    public let other: Other?
    /// Every diff file (the `Show complete diff` row); nil without a diff.
    public let complete: GuideChangeSet?
}

/// A Changes row's muted count: `1 file`, `N files`.
public func guideFileCountLabel(_ count: Int) -> String {
    count == 1 ? "1 file" : "\(count) files"
}

/// True when a listed path names this diff file (its path or rename source).
public func guidePathMatches(_ path: String, _ file: Diff.File) -> Bool {
    file.path == path || (file.previousPath.map { !$0.isEmpty && $0 == path } ?? false)
}

/// The diff files a topic's paths name, in LISTED order (a file once), plus
/// the listed paths no diff file matches.
public func guideFilesFor(
    _ paths: [String], _ diffFiles: [Diff.File]
) -> (files: [Diff.File], missing: [String]) {
    let (picked, missing) = guideFileIndices(paths, diffFiles)
    return (picked.map { diffFiles[$0] }, missing)
}

private func guideFileIndices(
    _ paths: [String], _ diffFiles: [Diff.File]
) -> (indices: [Int], missing: [String]) {
    var picked: [Int] = []
    var missing: [String] = []
    for path in paths {
        let hits = diffFiles.indices.filter { guidePathMatches(path, diffFiles[$0]) }
        if hits.isEmpty {
            missing.append(path)
            continue
        }
        for hit in hits where !picked.contains(hit) { picked.append(hit) }
    }
    return (picked, missing)
}

public func guideCoverage(_ groups: [SessionResultGroup], _ diffFiles: [Diff.File]?) -> GuideCoverage {
    let guide = sessionResultsGuide(groups)
    var claimed = Set<Int>()
    func cover(_ group: SessionResultGroup) -> (GuideChangeSet?, [String]) {
        guard let diffFiles else { return (nil, []) }
        let (indices, missing) = guideFileIndices(group.files, diffFiles)
        claimed.formUnion(indices)
        return (GuideChangeSet(files: indices.map { diffFiles[$0] }), missing)
    }
    let lead = guide.lead.map { group -> GuideCoveredGroup in
        let (changes, missing) = cover(group)
        return GuideCoveredGroup(group: group, changes: changes, missing: missing)
    }
    let sections = guide.sections.map { section -> GuideCoverageSection in
        let (changes, missing) = cover(section.group)
        return GuideCoverageSection(
            group: section.group, changes: changes, missing: missing,
            index: section.index, total: section.total
        )
    }
    guard let diffFiles else {
        return GuideCoverage(lead: lead, sections: sections, other: nil, complete: nil)
    }
    let rest = diffFiles.indices.filter { !claimed.contains($0) }.map { diffFiles[$0] }
    return GuideCoverage(
        lead: lead,
        sections: sections,
        other: rest.isEmpty ? nil : GuideCoverage.Other(
            topic: groups.isEmpty ? guideChangesTopic : guideOtherChangesTopic,
            changes: GuideChangeSet(files: rest)
        ),
        complete: GuideChangeSet(files: diffFiles)
    )
}

// MARK: - EXP-1154: the PR body standing in for a missing report

/// The fallback's band label without a PR title (web
/// `PR_DESCRIPTION_FALLBACK_TOPIC`).
public let prDescriptionFallbackTopic = "Pull request"
/// The fallback's body for a blank PR description (web `PR_DESCRIPTION_EMPTY`).
public let prDescriptionEmpty = "No description."

/// The GitHub PR body as ONE Guide group (web `prDescriptionGroups`, M8 ×4):
/// band = the PR title, else `Pull request`; `No description.` when blank.
/// No report + a PR = ONE Changes section, so the group claims EVERY diff
/// path: its band carries the one Changes row and nothing is left for
/// `Other changes`.
public func prDescriptionGroup(title: String?, body: String?, files: [Diff.File]?) -> SessionResultGroup {
    let topic = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    let text = body?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return SessionResultGroup(
        topic: topic.isEmpty ? prDescriptionFallbackTopic : topic,
        text: text.isEmpty ? prDescriptionEmpty : text,
        entries: [],
        files: (files ?? []).map(\.path)
    )
}

// MARK: - EXP-1251: a run's topics scoped per PR

// A run that stacks a second PR scopes its topics: a text entry may carry
// `prUrl`, and a PR (or the issue that owns it) shows only the topics tagged
// with it plus the untagged ones. Pictures follow their topic's text. Fixture
// `session-results.json` `prScope` (×4).

/// A topic's PR tag: its first non-blank text entry's `prUrl`, trimmed.
private func topicPrUrls(_ records: [[String: Any]]) -> (tags: [String: String], order: [String]) {
    var tags: [String: String] = [:]
    var order: [String] = []
    var seen = Set<String>()
    for record in records {
        if resultPicture(record) != nil { continue }
        guard let topic = resultText(record["topic"]),
              resultText(record["text"]) != nil,
              !seen.contains(topic)
        else { continue }
        seen.insert(topic)
        if let prUrl = resultText(record["prUrl"]) {
            tags[topic] = prUrl
            order.append(prUrl)
        }
    }
    return (tags, order)
}

/// The entries a PR shows: untagged topics and the ones tagged `prUrl`; feed
/// the answer to `parseSessionResultGroups(records:)`.
public func sessionResultsForPr(_ raw: String?, prUrl: String?) -> [[String: Any]] {
    let list = resultRecords(raw)
    let tags = topicPrUrls(list).tags
    let want = prUrl?.trimmingCharacters(in: .whitespacesAndNewlines)
    let wanted = (want?.isEmpty ?? true) ? nil : want
    return list.filter { record in
        guard let topic = resultText(record["topic"]), let tag = tags[topic] else { return true }
        return tag == wanted
    }
}

/// The groups a PR shows (`sessionResultsForPr` → the group reader).
public func sessionResultGroupsForPr(_ raw: String?, prUrl: String?) -> [SessionResultGroup] {
    parseSessionResultGroups(records: sessionResultsForPr(raw, prUrl: prUrl))
}

/// Every PR url a run's topics are tagged with, first-seen order.
public func sessionResultPrUrls(_ raw: String?) -> [String] {
    var seen = Set<String>()
    return topicPrUrls(resultRecords(raw)).order.filter { seen.insert($0).inserted }
}

/// True when the blob has anything for the Results face to show.
public func hasSessionResults(_ raw: String?) -> Bool {
    !parseSessionResultGroups(raw).isEmpty
}

/// Every picture of a set of groups, in order — what the tile sizing reads
/// (the folded `earlier` ones too: expanding the band never resizes).
public func sessionResultPictures(_ groups: [SessionResultGroup]) -> [SessionResultEntry] {
    groups.flatMap { $0.entries + $0.earlier }
}

/// EXP-1172: the picture an `exponential_sessions_show` call filed, by the
/// attachment id its answer carried (`preview.id`); nil while the upload is
/// still in flight, once it was removed, or for a blank id.
public func sessionResultPicture(_ raw: String?, attachmentId: String?) -> SessionResultEntry? {
    guard let id = attachmentId?.trimmingCharacters(in: .whitespacesAndNewlines),
          !id.isEmpty
    else { return nil }
    return parseSessionResults(raw).first { $0.attachmentId == id }
}

/// EXP-1128: true when the probed aspect is under `sessionResultTallAspect`;
/// an unmeasured picture is never tall.
public func sessionResultIsTall(_ entry: SessionResultEntry) -> Bool {
    guard let width = entry.width, let height = entry.height else { return false }
    return CGFloat(width) / CGFloat(height) < sessionResultTallAspect
}

/// EXP-1128: a tall picture's viewer draws it as horizontal STRIPS of at most
/// `rows` pixel rows each (Core Animation caps one layer's texture near 16384
/// px), top to bottom; the last strip is the remainder, 0 rows gives none.
public func tallImageStripRanges(height: Int, rows: Int = 4096) -> [(y: Int, rows: Int)] {
    guard height > 0, rows > 0 else { return [] }
    return stride(from: 0, to: height, by: rows).map { y in (y: y, rows: min(rows, height - y)) }
}

/// The tile's width at a fixed height — the probed aspect, else 4:3 (a desktop
/// screenshot's shape, and the least surprising placeholder). A TALL picture
/// (EXP-1128) takes the 4:3 frame too: the tile shows its top, never a sliver.
public func sessionResultTileWidth(
    _ entry: SessionResultEntry,
    height: CGFloat = sessionResultTileHeight
) -> CGFloat {
    let aspect: CGFloat
    if let width = entry.width, let entryHeight = entry.height, !sessionResultIsTall(entry) {
        aspect = CGFloat(width) / CGFloat(entryHeight)
    } else {
        aspect = 4.0 / 3.0
    }
    return (height * aspect).rounded()
}

/// The height EVERY tile on the page renders at, scaled down by ONE factor
/// when the widest tile would not fit the page's content width.
///
/// A phone is narrower than a single landscape tile is wide at the pinned
/// 320pt height (a 16:9 shot is 569pt), so without this the strip would either
/// clip or wrap to one tile per row. Scaling is applied to the whole page, not
/// per tile: every tile keeps its probed aspect AND its shared height, which is
/// the point of reading a row of an iOS, an Android and a web shot as one
/// strip.
public func sessionResultTileHeightFitting(
    _ entries: [SessionResultEntry],
    availableWidth: CGFloat,
    base: CGFloat = sessionResultTileHeight
) -> CGFloat {
    guard availableWidth.isFinite, availableWidth > 0 else { return base }
    let widest = entries.map { sessionResultTileWidth($0, height: base) }.max() ?? 0
    guard widest > availableWidth else { return base }
    return max(1, (base * availableWidth / widest).rounded(.down))
}

// MARK: - EXP-1175: the Run face's THREAD

/// What the run published, in the order it published it, with the Summary
/// text as the agent's reply at the end. Fixture `session-results.json`
/// `thread` (×4).
public struct SessionThread: Equatable, Sendable {
    public enum Item: Equatable, Sendable {
        case text(topic: String, text: String)
        case picture(SessionResultEntry)
    }

    public let items: [Item]
    /// The Summary topic's text (its first non-blank one), drawn LAST as the
    /// agent's reply; nil without one.
    public let reply: String?

    public init(items: [Item] = [], reply: String? = nil) {
        self.items = items
        self.reply = reply
    }

    public var isEmpty: Bool { items.isEmpty && reply == nil }
}

/// A run's thread in publish order, each piece with the server's write stamp
/// (`at`, ms; nil on an entry filed before EXP-1251).
private struct ThreadPiece {
    let item: SessionThread.Item?
    let reply: String?
    let at: Double?
}

private func stampMs(_ value: Any?) -> Double? {
    guard let number = value as? NSNumber,
          CFGetTypeID(number) != CFBooleanGetTypeID()
    else { return nil }
    let double = number.doubleValue
    return double.isFinite && double > 0 ? double : nil
}

private func threadPieces(_ raw: String?) -> [ThreadPiece] {
    var pieces: [ThreadPiece] = []
    var seenText = Set<String>()
    var replied = false
    var pictures = 0
    for record in resultRecords(raw) {
        let at = stampMs(record["at"])
        if let entry = resultPicture(record) {
            if pictures >= maxSessionResults { continue }
            pictures += 1
            pieces.append(ThreadPiece(item: .picture(entry), reply: nil, at: at))
            continue
        }
        guard let topic = resultText(record["topic"]),
              let body = resultText(record["text"])
        else { continue }
        if isSummaryTopic(topic) {
            if !replied { pieces.append(ThreadPiece(item: nil, reply: body, at: at)) }
            replied = true
            continue
        }
        if seenText.contains(topic) { continue }
        seenText.insert(topic)
        pieces.append(ThreadPiece(item: .text(topic: topic, text: body), reply: nil, at: at))
    }
    return pieces
}

/// Array (publish) order, the group reader's tolerance and 60-picture cap: a
/// picture is an item where it sits; a topic's FIRST non-blank text is an item
/// where it sits (a later text of the same topic is dropped); the Summary
/// topic's text is `reply`, never an item, while pictures filed under Summary
/// stay in the stream.
public func sessionThread(_ raw: String?) -> SessionThread {
    var items: [SessionThread.Item] = []
    var reply: String?
    for piece in threadPieces(raw) {
        if let item = piece.item { items.append(item) } else { reply = piece.reply }
    }
    return SessionThread(items: items, reply: reply)
}

// MARK: - EXP-1245: the thread as a CONVERSATION of turns

// For the run's OWNER (the relay feed is theirs alone): each turn = the
// person's message that opened it (a bubble), its status row (`startedAt` /
// `endedAt` feed the per-turn caption) and the results the server stamped
// (`at`) inside it, the Summary as the reply of the turn that last wrote it.
// Teammates and an offline host have no feed and keep today's single-row
// thread. Fixture `session-results.json` `turns` (×4).

/// One relay feed fact the turns read: a person's message or a turn edge.
/// Times are epoch milliseconds.
public enum SessionTurnEvent: Equatable, Sendable {
    /// Wave D: `files` = the message's non-image attachments (iOS-local:
    /// the fixture carries none, so it defaults empty).
    case userMessage(at: Double, text: String, images: [String] = [], files: [SteerImageMessage.File] = [])
    case turn(started: Bool, at: Double)

    var at: Double {
        switch self {
        case let .userMessage(at, _, _, _): at
        case let .turn(_, at): at
        }
    }
}

public struct SessionTurnMessage: Equatable, Sendable {
    public let text: String
    public let at: Double
    /// The message's image urls, in order (the bubble shows a thumb).
    public let images: [String]
    /// Wave D: the message's non-image files, in order (link rows).
    public let files: [SteerImageMessage.File]

    public init(text: String, at: Double, images: [String] = [], files: [SteerImageMessage.File] = []) {
        self.text = text
        self.at = at
        self.images = images
        self.files = files
    }
}

public struct SessionTurn: Equatable, Sendable {
    /// The person's message that opened the turn; nil for the first turn of
    /// an issue run (its prompt is the issue) or a turn the agent began.
    public var message: SessionTurnMessage?
    /// The turn's `started` edge; nil while a sent message waits.
    public var startedAt: Double?
    /// The turn's `ended` edge (or the next message that cut in); nil while live.
    public var endedAt: Double?
    public var items: [SessionThread.Item]
    public var reply: String?

    public init(
        message: SessionTurnMessage? = nil,
        startedAt: Double? = nil,
        endedAt: Double? = nil,
        items: [SessionThread.Item] = [],
        reply: String? = nil
    ) {
        self.message = message
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.items = items
        self.reply = reply
    }

    var boundary: Double { message?.at ?? startedAt ?? 0 }
}

public struct SessionTurns: Equatable, Sendable {
    /// False = no feed: ONE turn holding today's thread, drawn under the one
    /// run-wide status row.
    public let perTurn: Bool
    public let turns: [SessionTurn]
}

/// Walks the feed in time order (ties keep feed order): a message opens a new
/// turn (closing a still-open one at its time, the new turn starting there
/// too: the agent never stopped); a `started` edge starts the newest turn when
/// it has not started yet, else opens a turn with no message; an `ended` edge
/// ends the open turn. Every result lands in the LAST turn whose boundary
/// (message time, else start) is at or before its `at`; one without `at`, or
/// older than the first turn, lands in the first. No usable feed event = one
/// turn, `perTurn` false.
public func sessionTurns(_ raw: String?, feed: [SessionTurnEvent]? = nil) -> SessionTurns {
    let events = (feed ?? []).enumerated()
        .filter { $0.element.at.isFinite }
        .sorted { a, b in
            a.element.at != b.element.at ? a.element.at < b.element.at : a.offset < b.offset
        }
        .map(\.element)
    var turns: [SessionTurn] = []
    func openIndex() -> Int? {
        guard let last = turns.indices.last,
              turns[last].startedAt != nil, turns[last].endedAt == nil
        else { return nil }
        return last
    }
    for event in events {
        switch event {
        case let .userMessage(at, text, images, files):
            let running = openIndex()
            if let running { turns[running].endedAt = at }
            turns.append(SessionTurn(
                message: SessionTurnMessage(
                    text: text, at: at, images: images.filter { !$0.isEmpty }, files: files
                ),
                startedAt: running == nil ? nil : at
            ))
        case let .turn(started, at):
            if started {
                if let last = turns.indices.last,
                   turns[last].startedAt == nil, turns[last].endedAt == nil {
                    turns[last].startedAt = at
                } else if openIndex() == nil {
                    turns.append(SessionTurn(startedAt: at))
                }
            } else if let running = openIndex() {
                turns[running].endedAt = at
            }
        }
    }
    if turns.isEmpty {
        let thread = sessionThread(raw)
        return SessionTurns(
            perTurn: false,
            turns: [SessionTurn(items: thread.items, reply: thread.reply)]
        )
    }
    for piece in threadPieces(raw) {
        var target = 0
        if let at = piece.at {
            for index in turns.indices where turns[index].boundary <= at { target = index }
        }
        if let item = piece.item {
            turns[target].items.append(item)
        } else {
            turns[target].reply = piece.reply
        }
    }
    return SessionTurns(perTurn: true, turns: turns)
}

// MARK: - EXP-1245: the owner's message bubble caption

/// `21:40`: local hours and minutes, both two-digit (web `userMessageTime`).
public func userMessageTime(_ atMs: Double, timeZone: TimeZone = .current) -> String {
    guard atMs.isFinite else { return "" }
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = timeZone
    let parts = calendar.dateComponents([.hour, .minute], from: Date(timeIntervalSince1970: atMs / 1000))
    return String(format: "%02d:%02d", parts.hour ?? 0, parts.minute ?? 0)
}

/// The caption under the bubble — `<name> · <time> · from <device>`; a
/// missing part drops with its separator (web `userMessageCaption`).
public func userMessageCaption(
    name: String?, at: Double?, device: String?, timeZone: TimeZone = .current
) -> String {
    var parts: [String] = []
    if let name = name?.trimmingCharacters(in: .whitespacesAndNewlines), !name.isEmpty {
        parts.append(name)
    }
    if let at {
        let time = userMessageTime(at, timeZone: timeZone)
        if !time.isEmpty { parts.append(time) }
    }
    if let device = device?.trimmingCharacters(in: .whitespacesAndNewlines), !device.isEmpty {
        parts.append("from \(device)")
    }
    return parts.joined(separator: " · ")
}
