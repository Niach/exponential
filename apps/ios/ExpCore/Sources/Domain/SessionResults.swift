import CoreGraphics
import Foundation

// EXP-879: a coding run's published RESULTS — the screenshots the agent filed
// with `exponential_sessions_results` while it worked, read off the synced
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

    public init(
        topic: String,
        text: String? = nil,
        entries: [SessionResultEntry],
        earlier: [SessionResultEntry] = [],
        files: [String] = []
    ) {
        self.topic = topic
        self.text = text
        self.entries = entries
        self.earlier = earlier
        self.files = files
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
        files: group.files
    )
}

/// Groups by topic in FIRST-SEEN order, keeping each group's entries in the
/// order the agent published them.
public func groupSessionResults(
    _ entries: [SessionResultEntry]
) -> [SessionResultGroup] {
    var topics: [String] = []
    var byTopic: [String: [SessionResultEntry]] = [:]
    for entry in entries {
        if byTopic[entry.topic] == nil {
            topics.append(entry.topic)
            byTopic[entry.topic] = []
        }
        byTopic[entry.topic]?.append(entry)
    }
    return topics.map { foldInline(SessionResultGroup(topic: $0, entries: byTopic[$0] ?? [])) }
}

/// EXP-933: the Results face as a REPORT — pictures AND each topic's text
/// (`{topic, label: null, attachmentId: null, text}`), grouped in FIRST-SEEN
/// topic order whichever kind opened the topic. A topic's text is its FIRST
/// non-blank text entry, trimmed; pictures keep the 60 cap. Fixture:
/// `packages/domain-contract/fixtures/session-results.json` (×4).
public func parseSessionResultGroups(_ raw: String?) -> [SessionResultGroup] {
    var topics: [String] = []
    var texts: [String: String] = [:]
    var filesByTopic: [String: [String]] = [:]
    var byTopic: [String: [SessionResultEntry]] = [:]
    func open(_ topic: String) {
        if byTopic[topic] == nil {
            topics.append(topic)
            byTopic[topic] = []
        }
    }
    var pictures = 0
    for record in resultRecords(raw) {
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
        }
    }
    return topics.map {
        foldInline(SessionResultGroup(
            topic: $0,
            text: texts[$0],
            entries: byTopic[$0] ?? [],
            files: filesByTopic[$0] ?? []
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

/// One file row under a Guide section: counts only when the path matches a
/// loaded diff file exactly.
public struct GuideFileRow: Equatable, Sendable, Identifiable {
    public let path: String
    public let additions: Int?
    public let deletions: Int?

    public init(path: String, additions: Int? = nil, deletions: Int? = nil) {
        self.path = path
        self.additions = additions
        self.deletions = deletions
    }

    public var id: String { path }
}

/// One row per path in order. Fixture `guide.fileRows` (×4).
public func guideFileRows(_ paths: [String], files: [Diff.File]?) -> [GuideFileRow] {
    paths.map { path in
        guard let file = files?.first(where: { $0.path == path }) else {
            return GuideFileRow(path: path)
        }
        return GuideFileRow(path: path, additions: file.additions, deletions: file.deletions)
    }
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
