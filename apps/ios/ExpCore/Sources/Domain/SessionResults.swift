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

public struct SessionResultEntry: Equatable, Sendable {
    public let topic: String
    public let label: String
    public let attachmentId: String
    /// Probed at upload; nil when the image could not be measured.
    public let width: Int?
    public let height: Int?

    public init(
        topic: String,
        label: String,
        attachmentId: String,
        width: Int? = nil,
        height: Int? = nil
    ) {
        self.topic = topic
        self.label = label
        self.attachmentId = attachmentId
        self.width = width
        self.height = height
    }

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

/// Tolerant reader for the jsonb column as the entity stores it: raw JSON
/// text. nil, blank and unparseable all read as "no results", and a malformed
/// entry is DROPPED rather than rendered as a broken tile. The list is capped
/// exactly like the writer caps it.
public func parseSessionResults(_ raw: String?) -> [SessionResultEntry] {
    guard let raw else { return [] }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty,
          let data = trimmed.data(using: .utf8),
          let object = try? JSONSerialization.jsonObject(with: data),
          let rows = object as? [Any]
    else { return [] }

    var entries: [SessionResultEntry] = []
    for row in rows {
        guard let record = row as? [String: Any],
              let topic = resultText(record["topic"]),
              let label = resultText(record["label"]),
              let attachmentId = resultText(record["attachmentId"])
        else { continue }
        entries.append(
            SessionResultEntry(
                topic: topic,
                label: label,
                attachmentId: attachmentId,
                width: resultDimension(record["width"]),
                height: resultDimension(record["height"])
            )
        )
        if entries.count >= maxSessionResults { break }
    }
    return entries
}

public struct SessionResultGroup: Equatable, Sendable, Identifiable {
    public let topic: String
    public let entries: [SessionResultEntry]

    public init(topic: String, entries: [SessionResultEntry]) {
        self.topic = topic
        self.entries = entries
    }

    public var id: String { topic }
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
    return topics.map { SessionResultGroup(topic: $0, entries: byTopic[$0] ?? []) }
}

/// The tile's width at a fixed height — the probed aspect, else 4:3 (a desktop
/// screenshot's shape, and the least surprising placeholder).
public func sessionResultTileWidth(
    _ entry: SessionResultEntry,
    height: CGFloat = sessionResultTileHeight
) -> CGFloat {
    let aspect: CGFloat
    if let width = entry.width, let entryHeight = entry.height {
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
