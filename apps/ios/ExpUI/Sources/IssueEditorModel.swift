import ExpCore
import Foundation
import os

private let log = Logger(subsystem: "com.exponential", category: "IssueEditorModel")

/// Why an inline image upload failed — drives the failed-tile copy.
public enum ImageUploadFailureReason: Equatable, Sendable {
    /// The server's storage cap (HTTP 412 from the images route) — retrying
    /// keeps failing until the team frees space, so the tile explains why.
    case storageFull
    case other
}

/// Upload state for an inline image block, surfaced as status / retry UI.
public enum ImageUploadState: Equatable, Sendable {
    case idle
    case uploading
    case failed(ImageUploadFailureReason)
}

/// Classify an upload error for the failed tile: the images route answers the
/// team storage cap with HTTP 412 (its body carries billing copy that must
/// never render — EXP-216).
private func uploadFailureReason(_ error: any Error) -> ImageUploadFailureReason {
    if case IssueImagesError.httpError(412, _) = error { return .storageFull }
    return .other
}

/// A team member the @-autocomplete can offer. The canonical interchange
/// form is the `@email` token, so `id` is the email.
public struct MentionMember: Identifiable, Sendable, Equatable {
    public let name: String
    public let email: String
    public var id: String { email }
    public init(name: String, email: String) {
        self.name = name
        self.email = email
    }
}

/// An issue the #-autocomplete can offer. The canonical interchange form is
/// the plain `#IDENTIFIER` token (mirrors `MentionMember` for @mentions), so
/// `id` is the identifier.
public struct IssueRefCandidate: Identifiable, Sendable, Equatable {
    public let identifier: String
    public let title: String
    /// EXP-581: the issue's resolved team status, so the autocomplete row can
    /// lead with the same status glyph the web/Android rows show. Nil renders
    /// the row without a glyph (tests, hosts without a status read).
    public let status: ResolvedIssueStatus?
    public var id: String { identifier }
    public init(identifier: String, title: String, status: ResolvedIssueStatus? = nil) {
        self.identifier = identifier
        self.title = title
        self.status = status
    }
}

/// Single source of truth for the block-based markdown editor. The editor view
/// renders `blocks` and routes every edit through this model; markdown is
/// *derived* (`currentMarkdown()`) only at save points, never round-tripped on
/// each keystroke. This eliminates the old `@State blocks` + `@Binding text`
/// race and the per-keystroke cmark re-serialization.
@MainActor
@Observable
public final class IssueEditorModel {
    /// The document, as ordered text/image blocks. Mutated only through the
    /// intent methods below so revisions and pending state stay consistent.
    public private(set) var blocks: [ContentBlock] = []

    /// In-memory image data keyed by its `draft://` placeholder URL, kept until
    /// the image is uploaded and its block URL swapped to the real attachment.
    public var pendingImages: [String: PendingImage] = [:]

    /// Currently focused text block, drives first-responder + autosave-on-blur.
    public private(set) var focusedBlockId: UUID?

    /// A remote (Electric) update that arrived while the user was actively
    /// editing or had unsaved edits — surfaced as a non-blocking reload banner.
    public private(set) var pendingRemoteMarkdown: String?

    /// Markdown last persisted to the server, used to detect local dirtiness.
    public private(set) var lastSavedMarkdown: String = ""

    /// Per-image upload state for inline status / retry affordances.
    public private(set) var imageUploadStates: [UUID: ImageUploadState] = [:]

    /// Caret the editor should move to after the next structural mutation.
    public private(set) var desiredSelection: (blockId: UUID, location: Int)?

    /// Invoked on user-originated edits so the host can schedule a debounced save.
    public var onEdit: (() -> Void)?

    /// Team members the @-autocomplete can offer (set by the host; should be
    /// pre-filtered to non-agent members). Membership often syncs in AFTER the
    /// document loads, so a change re-runs the chip pass — otherwise mentions
    /// would stay plain until the next reload.
    public var mentionMembers: [MentionMember] = [] {
        didSet {
            guard mentionMembers != oldValue else { return }
            redecorateChips()
        }
    }

    /// Render-only resolver for inline `#IDENTIFIER` issue refs: identifier
    /// (e.g. `VER-12`) → issue id, from the host's local store. When set,
    /// `load()` decorates resolved tokens as tappable pills (unknown
    /// identifiers stay plain text). Purely display — the derived markdown is
    /// byte-identical either way.
    public var issueRefResolver: ((String) -> String?)?

    /// Identifier → issue TITLE, so a resolved `#IDENTIFIER` chip reads
    /// `#ID <title>` like the web editor (EXP-307, EXP-322).
    ///
    /// Safe on EDITABLE models: the title rides a single `NSTextAttachment`
    /// character, and `MarkdownConversion`'s serializer skips `.attachment`
    /// runs, so the derived markdown keeps the bare token. (Read-only DISPLAY
    /// models take a different route — `IssueRefs.decorateForDisplay` splices
    /// the title in as real characters, which is fine there because they never
    /// serialize.)
    public var issueRefTitleResolver: ((String) -> String?)?

    /// Identifier → the issue's resolved STATUS (glyph + tint), which the chip
    /// paints over its `#` cell (EXP-423, Linear parity). Purely a
    /// `.foregroundColor` + attribute change, so — like the resolvers above —
    /// the derived markdown is byte-identical either way.
    public var issueRefStatusResolver: ((String) -> IssueRefStatusInfo?)?

    /// Read-only comment cards set this so the chip title is spliced in as
    /// text (`IssueRefs.decorateForDisplay`) instead of riding an attachment.
    /// Never set it on a model whose markdown gets saved.
    public var isDisplayOnly = false

    /// Issue search backing the #-autocomplete (set by the host; team-
    /// scoped, matching identifier + title substrings — empty query = most
    /// recent). nil disables the #-autocomplete.
    public var issueRefSearch: ((String) -> [IssueRefCandidate])?

    /// EXP-551 — emoji search backing the `:shortcode` typeahead (set by the
    /// host from `EmojiCatalog`). nil disables it, which is also the state
    /// while the bundled dataset is still decoding off-main.
    public var emojiSearch: ((String) -> [EmojiRecord])?

    /// EXP-551 — the user's skin-tone preference (0 = none, 1...5 light to
    /// dark), applied to every emoji this model inserts. Hosts mirror
    /// `EmojiPreferences.skinTone` into it.
    public var emojiSkinTone: Int = 0

    /// EXP-551 — fired after an emoji is committed from the typeahead so the
    /// host can record it in the shared recents list.
    public var onEmojiInserted: ((EmojiRecord) -> Void)?

    /// Active @-mention candidates for the focused block's caret query, recomputed
    /// on edit/selection. Empty when no mention token is being typed.
    public private(set) var mentionCandidates: [MentionMember] = []

    /// Active #-issue-ref candidates for the focused block's caret query,
    /// recomputed on edit/selection. Empty when no #-token is being typed.
    public private(set) var issueRefCandidates: [IssueRefCandidate] = []

    /// Active `:shortcode` emoji candidates for the focused block's caret
    /// query, recomputed on edit/selection (EXP-551).
    public private(set) var emojiCandidates: [EmojiRecord] = []

    /// EXP-581: the text block an OPEN autocomplete menu belongs to, so the
    /// editor can anchor the menu right under that block instead of painting
    /// a bar over the editor's top edge. Nil when no menu is open.
    public var autocompleteAnchorBlockId: UUID? {
        if !mentionCandidates.isEmpty { return activeMention?.blockId }
        if !issueRefCandidates.isEmpty { return activeIssueRef?.blockId }
        if !emojiCandidates.isEmpty { return activeEmoji?.blockId }
        return nil
    }

    /// EXP-581: whether any autocomplete menu currently has candidates.
    public var hasAutocompleteCandidates: Bool {
        !mentionCandidates.isEmpty || !issueRefCandidates.isEmpty || !emojiCandidates.isEmpty
    }

    // The @-token currently being edited: where the `@` is and how long the query
    // after it is, in the focused block.
    private var activeMention: (blockId: UUID, atOffset: Int, queryLength: Int)?

    // The #-token currently being edited (mirrors `activeMention`).
    private var activeIssueRef: (blockId: UUID, hashOffset: Int, queryLength: Int)?

    // The `:shortcode` token currently being edited. `closingColonLength` is 1
    // once the user typed the closing `:`, so the replacement covers it.
    private var activeEmoji: (blockId: UUID, colonOffset: Int, queryLength: Int, closingColonLength: Int)?

    // An @mention at the caret: `@` preceded by start-of-text or whitespace, then
    // an email-ish run. Mirrors the web/Android regex.
    private static let mentionRegex = try! NSRegularExpression(
        pattern: "(?:^|\\s)@([A-Za-z0-9._%+-]*)$"
    )

    // A #issue-ref at the caret: `#` preceded by start-of-text or whitespace,
    // then an identifier-ish run. Mirrors the web `ISSUE_REF_AT_CARET`.
    private static let issueRefAtCaretRegex = try! NSRegularExpression(
        pattern: "(?:^|\\s)#([A-Za-z0-9-]*)$"
    )

    // EXP-551 — a `:shortcode` at the caret. Byte-identical to the shared
    // trigger every client uses (`packages/emoji/README.md`): the colon must
    // start the line or follow whitespace, at least two shortcode characters
    // must follow, and a closing colon is optional. `12:30`, `note:`, `http://x`
    // and `:)` deliberately never trigger.
    private static let emojiAtCaretRegex = try! NSRegularExpression(
        pattern: "(?:^|\\s):([a-z0-9_+-]{2,})(:?)$",
        options: [.caseInsensitive]
    )

    /// If `text` (the focused block's content up to the caret) ends in an @mention
    /// token, return the query (text after `@`) and the `@` character offset.
    public static func mentionMatch(beforeCaret text: String) -> (query: String, atOffset: Int)? {
        let ns = text as NSString
        guard let m = mentionRegex.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)) else {
            return nil
        }
        let q = m.range(at: 1)
        return (ns.substring(with: q), q.location - 1)
    }

    /// If `text` (the focused block's content up to the caret) ends in a #issue-ref
    /// token, return the query (text after `#`) and the `#` character offset.
    public static func issueRefMatch(beforeCaret text: String) -> (query: String, hashOffset: Int)? {
        let ns = text as NSString
        guard let m = issueRefAtCaretRegex.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)) else {
            return nil
        }
        let q = m.range(at: 1)
        return (ns.substring(with: q), q.location - 1)
    }

    /// If `text` (the focused block's content up to the caret) ends in a
    /// `:shortcode` token, return the query (text after the `:`), the opening
    /// `:` character offset and whether the closing `:` has been typed
    /// (EXP-551).
    public static func emojiMatch(beforeCaret text: String) -> (query: String, colonOffset: Int, closed: Bool)? {
        let ns = text as NSString
        guard let m = emojiAtCaretRegex.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)) else {
            return nil
        }
        let q = m.range(at: 1)
        return (ns.substring(with: q), q.location - 1, m.range(at: 2).length == 1)
    }

    // Monotonic content revisions, bumped ONLY for external/structural changes
    // (load, remote apply, insert, delete, merge) — never for the user's own
    // keystrokes — so a text view never clobbers what the user just typed.
    private var revisions: [UUID: Int] = [:]
    private var revisionCounter = 0

    // Last known caret in the focused text block, used for image insertion.
    private var selection: (blockId: UUID, range: NSRange)?

    public init() {
        // Always start with one empty text block so a fresh editor (e.g. the
        // create sheet, before any load) is immediately typeable.
        let id = UUID()
        blocks = [.text(id: id, attributedContent: NSAttributedString())]
        revisions[id] = 0
    }

    // MARK: - Derived state

    public func currentMarkdown() -> String {
        MarkdownConversion.blocksToMarkdown(blocks)
    }

    public var isEditing: Bool { focusedBlockId != nil }
    public var isDirty: Bool { currentMarkdown() != lastSavedMarkdown }
    public var hasUncommittedDrafts: Bool { MarkdownImageUtils.hasDraftImages(currentMarkdown()) }

    public func revision(for id: UUID) -> Int { revisions[id] ?? 0 }
    public func uploadState(for id: UUID) -> ImageUploadState { imageUploadStates[id] ?? .idle }

    // MARK: - Loading / reconciliation

    /// `options` deviates from the interchange contract and is therefore only
    /// ever passed by display-only models (EXP-440) — `applyRemote` and
    /// `reloadPendingRemote` deliberately keep the default, since a model that
    /// can serialize must parse exactly what it saves.
    public func load(markdown: String, baseURL: URL?, options: MarkdownParseOptions = []) {
        blocks = MarkdownConversion.markdownToBlocks(markdown, baseURL: baseURL, options: options)
        decorateChips()
        bumpAllRevisions()
        // Baseline against the DERIVED markdown, not the raw input: the
        // markdown↔blocks round-trip is not byte-identical, so using the raw
        // input here would make the editor read as "dirty" immediately.
        lastSavedMarkdown = currentMarkdown()
        pendingRemoteMarkdown = nil
        focusedBlockId = nil
        selection = nil
    }

    /// Apply a remote markdown update if safe (not actively editing, no unsaved
    /// local edits). Otherwise stash it for a user-driven reload (LWW + banner).
    public func applyRemote(markdown: String, baseURL: URL?) {
        // Normalize the remote text through the same block round-trip so
        // cosmetic serialization differences don't read as real changes.
        let normalizedRemote = MarkdownConversion.blocksToMarkdown(
            MarkdownConversion.markdownToBlocks(markdown, baseURL: baseURL)
        )
        if normalizedRemote == currentMarkdown() {
            lastSavedMarkdown = normalizedRemote
            pendingRemoteMarkdown = nil
            return
        }
        if !isEditing, !isDirty {
            load(markdown: markdown, baseURL: baseURL)
        } else {
            pendingRemoteMarkdown = markdown
        }
    }

    public func reloadPendingRemote(baseURL: URL?) {
        guard let pending = pendingRemoteMarkdown else { return }
        load(markdown: pending, baseURL: baseURL)
    }

    /// Decorate resolved `#IDENTIFIER` and `@email` tokens in every text block
    /// (render-only; see `issueRefResolver`). Called from `load()` so remote
    /// applies and user-driven reloads re-decorate too — the live per-keystroke
    /// pass goes through [chipDecoration] and produces identical output.
    private func decorateChips() {
        for (idx, block) in blocks.enumerated() {
            guard case let .text(id, content) = block else { continue }
            let result = chipDecoration(for: content, selection: NSRange(location: 0, length: 0))
            if result.changed {
                blocks[idx] = .text(id: id, attributedContent: result.attributed)
            }
        }
    }

    /// Like [decorateChips], but for a live document: bumps the revision of
    /// every block whose rendering actually changed so the text views re-apply.
    ///
    /// Safe to re-run on DISPLAY-ONLY models too, even though their route
    /// (`IssueRefs.decorateForDisplay`) splices the title in as real
    /// characters: that pass skips tokens that already carry
    /// `.markdownIssueRef`, so a second run can no longer append the title
    /// twice. Skipping display-only models entirely would be the other fix, but
    /// it would also stop late-syncing team members from ever chipping their
    /// `@email` mentions on a read-only comment card (EXP-322).
    private func redecorateChips() {
        for (idx, block) in blocks.enumerated() {
            guard case let .text(id, content) = block else { continue }
            let result = chipDecoration(for: content, selection: NSRange(location: 0, length: 0))
            guard result.changed else { continue }
            blocks[idx] = .text(id: id, attributedContent: result.attributed)
            bumpRevision(id)
        }
    }

    /// Email → display name for the mention chips, from the synced member list.
    private func mentionName(for email: String) -> String? {
        let needle = email.lowercased()
        return mentionMembers.first { $0.email.lowercased() == needle }?.name
    }

    /// The one place that assembles the resolvers for a chip pass, so the
    /// live editor pass and the `load()` pass can never drift. Read-only
    /// display models still splice the title in as text; editable models get
    /// the serialization-invisible attachment.
    public func chipDecoration(
        for content: NSAttributedString,
        selection: NSRange
    ) -> MarkdownChipDecorator.Result {
        if isDisplayOnly, let issueRefResolver, let issueRefTitleResolver {
            let decorated = IssueRefs.decorateForDisplay(
                content,
                resolver: issueRefResolver,
                titleResolver: issueRefTitleResolver,
                statusResolver: issueRefStatusResolver)
            let mentioned = mentionMembers.isEmpty
                ? decorated
                : MentionRefs.decorate(decorated) { [weak self] in self?.mentionName(for: $0) }
            return MarkdownChipDecorator.Result(
                attributed: mentioned,
                selection: selection,
                changed: !mentioned.isEqual(content),
            )
        }
        return MarkdownChipDecorator.decorate(
            content,
            selection: selection,
            issueRefResolver: issueRefResolver,
            issueRefTitleResolver: issueRefTitleResolver,
            issueRefStatusResolver: issueRefStatusResolver,
            mentionResolver: mentionMembers.isEmpty
                ? nil
                : { [weak self] in self?.mentionName(for: $0) },
        )
    }

    /// Apply a decoration pass's output to a block. Deliberately does NOT bump
    /// the revision (the text view already holds this content, and a bump would
    /// re-apply `attributedText` and disturb the caret / first responder), does
    /// not recompute the autocomplete, does not arm it, and does not
    /// `notifyEdit()` — a decoration pass never changes the markdown, so it
    /// must not schedule an autosave.
    ///
    /// It DOES move offsets, though: inserting or removing a display-only title
    /// attachment shifts every character after it. The text view suppresses its
    /// selection callback while it applies the pass, so the caller passes the
    /// mapped `selection` and the net `lengthDelta` here — otherwise the model
    /// would keep pre-pass coordinates for a post-pass document and the next
    /// candidate tap / image insert would land one character off (EXP-322).
    public func applyDecoration(
        id: UUID,
        content: NSAttributedString,
        selection: NSRange? = nil,
        lengthDelta: Int = 0
    ) {
        guard let idx = blocks.firstIndex(where: { $0.id == id }) else { return }
        blocks[idx] = .text(id: id, attributedContent: content)
        if let selection { self.selection = (id, selection) }
        // The active `@`/`#`/`:` token's offset is now stale. Remapping it would
        // mean replaying every insertion, and the bar is cheap to reopen (the
        // next keystroke arms it again) — whereas a stale replace range EATS
        // the character before the trigger, which then gets saved. Close it.
        if lengthDelta != 0 {
            clearMention()
            clearIssueRef()
            clearEmoji()
        }
    }

    public func markSaved(_ markdown: String) {
        lastSavedMarkdown = markdown
        if pendingRemoteMarkdown == markdown { pendingRemoteMarkdown = nil }
    }

    // MARK: - Focus / selection (reported by the text views)

    public func setFocused(_ id: UUID?) { focusedBlockId = id }

    public func clearFocusIfMatches(_ id: UUID) {
        if focusedBlockId == id { focusedBlockId = nil }
    }

    public func updateSelection(blockId: UUID, range: NSRange) {
        selection = (blockId, range)
        // The caret is fresh here, so this is the recompute that decides
        // whether the bar opens; consume the latch afterwards.
        recomputeAutocomplete(armed: textChangeArm)
        textChangeArm = false
    }

    /// The post-mutation caret location for `id`, consumed once.
    public func consumeDesiredSelection(for id: UUID) -> Int? {
        guard desiredSelection?.blockId == id else { return nil }
        let location = desiredSelection?.location
        desiredSelection = nil
        return location
    }

    // MARK: - Text editing

    public func updateText(id: UUID, content: NSAttributedString) {
        guard let idx = blocks.firstIndex(where: { $0.id == id }) else { return }
        // No revision bump: the originating text view already holds this content.
        blocks[idx] = .text(id: id, attributedContent: content)
        // Only a document change may OPEN an autocomplete bar. UIKit does not
        // guarantee whether the text or the selection callback lands first, so
        // arm a latch here and let whichever recompute sees the settled caret
        // use it (EXP-322).
        armTextChange()
        recomputeAutocomplete(armed: true)
        notifyEdit()
    }

    // MARK: - Mentions / issue refs (autocomplete)

    /// Set by [armTextChange], consumed by [updateSelection] or — when no
    /// selection callback follows the edit — by the end of the runloop turn.
    private var textChangeArm = false

    /// Bumped by every arm so a stale disarm can never clear a newer one.
    private var textChangeArmGeneration = 0

    /// Arm the "a document change happened" latch for the settled-caret
    /// recompute, and drop it again at the end of this runloop turn.
    ///
    /// The latch MUST be turn-scoped: the editor has three paths that set
    /// `selectedRange` themselves and then call `textViewDidChange` by hand
    /// (chip-atom backspace, list continuation, list exit), and those produce
    /// no selection callback to consume it. A latch left armed re-opens the
    /// candidate bar on the next plain tap inside an existing `#EXP-238` —
    /// exactly the bug EXP-322 set out to fix (EXP-322).
    private func armTextChange() {
        textChangeArm = true
        textChangeArmGeneration &+= 1
        let generation = textChangeArmGeneration
        Task { [weak self] in
            guard let self, self.textChangeArmGeneration == generation else { return }
            self.textChangeArm = false
        }
    }

    /// Recompute the @-mention and #-issue-ref candidates from the focused
    /// block's caret context. Driven off the model's own `selection` + `blocks`,
    /// so no caret geometry is needed in the text views. The two token shapes
    /// are mutually exclusive (each trigger must follow start-of-text or
    /// whitespace), so at most one candidate list is non-empty.
    ///
    /// [armed] mirrors web's `docChanged` rule (`editor-autocomplete.ts`): a
    /// caret move alone may TRACK or CLOSE an open bar but never open one, so
    /// tapping inside an existing `#EXP-238` stays quiet.
    private func recomputeAutocomplete(armed: Bool) {
        guard let sel = selection,
              let block = blocks.first(where: { $0.id == sel.blockId }),
              case let .text(_, content) = block else {
            clearMention()
            clearIssueRef()
            clearEmoji()
            return
        }
        let caret = max(0, min(sel.range.location, content.length))
        let before = (content.string as NSString).substring(to: caret)
        recomputeMention(blockId: sel.blockId, beforeCaret: before, armed: armed)
        recomputeIssueRef(blockId: sel.blockId, beforeCaret: before, armed: armed)
        // LAST on purpose: the closed-colon auto-commit below mutates `blocks`,
        // so the other two must have read the pre-commit content already.
        recomputeEmoji(blockId: sel.blockId, beforeCaret: before, armed: armed)
    }

    private func recomputeMention(blockId: UUID, beforeCaret before: String, armed: Bool) {
        guard !mentionMembers.isEmpty,
              let match = Self.mentionMatch(beforeCaret: before) else {
            clearMention()
            return
        }
        guard armed || activeMention?.blockId == blockId else {
            clearMention()
            return
        }
        activeMention = (blockId, match.atOffset, match.query.count)
        let q = match.query.lowercased()
        mentionCandidates = mentionMembers
            .filter { q.isEmpty || $0.name.lowercased().contains(q) || $0.email.lowercased().contains(q) }
            .prefix(6)
            .map { $0 }
    }

    private func recomputeIssueRef(blockId: UUID, beforeCaret before: String, armed: Bool) {
        guard let issueRefSearch,
              let match = Self.issueRefMatch(beforeCaret: before) else {
            clearIssueRef()
            return
        }
        guard armed || activeIssueRef?.blockId == blockId else {
            clearIssueRef()
            return
        }
        activeIssueRef = (blockId, match.hashOffset, match.query.count)
        issueRefCandidates = issueRefSearch(match.query)
    }

    /// EXP-551 — `:shortcode` typeahead. Mirrors the mention/issue-ref
    /// recompute, plus the auto-commit every client implements: once the
    /// CLOSING colon has been typed and the query is an exact shortcode, the
    /// token is replaced immediately (no trailing space, no menu).
    private func recomputeEmoji(blockId: UUID, beforeCaret before: String, armed: Bool) {
        guard let emojiSearch, let match = Self.emojiMatch(beforeCaret: before) else {
            clearEmoji()
            return
        }
        guard armed || activeEmoji?.blockId == blockId else {
            clearEmoji()
            return
        }
        let closingColonLength = match.closed ? 1 : 0
        activeEmoji = (blockId, match.colonOffset, match.query.count, closingColonLength)
        let results = emojiSearch(match.query)
        if match.closed, let exact = results.first(where: { $0.hasShortcode(match.query) }) {
            emojiCandidates = []
            applyEmoji(exact, trailingSpace: false)
            return
        }
        // A closed `:code:` that resolves to nothing is just text the user
        // typed — never offer a menu for it.
        emojiCandidates = match.closed ? [] : results
        if emojiCandidates.isEmpty, match.closed { activeEmoji = nil }
    }

    private func clearMention() {
        activeMention = nil
        if !mentionCandidates.isEmpty { mentionCandidates = [] }
    }

    private func clearEmoji() {
        activeEmoji = nil
        if !emojiCandidates.isEmpty { emojiCandidates = [] }
    }

    private func clearIssueRef() {
        activeIssueRef = nil
        if !issueRefCandidates.isEmpty { issueRefCandidates = [] }
    }

    /// Replace the active `@query` token with the canonical `@email ` form and put
    /// the caret after it. Reuses the revision + desiredSelection machinery so the
    /// text view re-applies the content without losing first responder.
    public func applyMention(_ member: MentionMember) {
        guard let active = activeMention,
              let idx = blocks.firstIndex(where: { $0.id == active.blockId }),
              case let .text(id, content) = blocks[idx] else {
            clearMention()
            return
        }
        let token = "@\(member.email) "
        let replaceRange = NSRange(location: active.atOffset, length: 1 + active.queryLength)
        guard replaceRange.location >= 0, NSMaxRange(replaceRange) <= content.length else {
            clearMention()
            return
        }
        let mutable = NSMutableAttributedString(attributedString: content)
        mutable.replaceCharacters(
            in: replaceRange,
            with: NSAttributedString(string: token, attributes: MarkdownStyle.baseAttributes)
        )
        blocks[idx] = .text(id: id, attributedContent: mutable)
        bumpRevision(id)
        desiredSelection = (id, active.atOffset + (token as NSString).length)
        clearMention()
        notifyEdit()
    }

    /// Replace the active `#query` token with the canonical plain `#IDENTIFIER `
    /// interchange text and put the caret after it. The inserted token stays
    /// plain text (never a custom node), so the markdown round-trip is
    /// untouched; when it resolves, the block is re-decorated so the pill shows
    /// immediately (render-only — see `issueRefResolver`).
    public func applyIssueRef(_ candidate: IssueRefCandidate) {
        guard let active = activeIssueRef,
              let idx = blocks.firstIndex(where: { $0.id == active.blockId }),
              case let .text(id, content) = blocks[idx] else {
            clearIssueRef()
            return
        }
        let token = "#\(candidate.identifier) "
        let replaceRange = NSRange(location: active.hashOffset, length: 1 + active.queryLength)
        guard replaceRange.location >= 0, NSMaxRange(replaceRange) <= content.length else {
            clearIssueRef()
            return
        }
        let mutable = NSMutableAttributedString(attributedString: content)
        mutable.replaceCharacters(
            in: replaceRange,
            with: NSAttributedString(string: token, attributes: MarkdownStyle.baseAttributes)
        )
        var caret = NSRange(location: active.hashOffset + (token as NSString).length, length: 0)
        let decorated = chipDecoration(for: mutable, selection: caret)
        if decorated.changed { caret = decorated.selection }
        blocks[idx] = .text(id: id, attributedContent: decorated.attributed)
        bumpRevision(id)
        desiredSelection = (id, caret.location)
        clearIssueRef()
        notifyEdit()
    }

    /// EXP-551 — replace the active `:query` (plus its closing colon, when
    /// typed) with the emoji's UNICODE. Same revision + desiredSelection
    /// machinery as `applyMention`, so the text view re-applies without losing
    /// first responder and the markdown stays plain GFM.
    ///
    /// [trailingSpace] mirrors the shared contract: a MENU pick inserts
    /// `unicode + " "`, while the exact-`:code:` auto-commit inserts the bare
    /// unicode (the user already typed a delimiter).
    public func applyEmoji(_ record: EmojiRecord, trailingSpace: Bool = true) {
        guard let active = activeEmoji,
              let idx = blocks.firstIndex(where: { $0.id == active.blockId }),
              case let .text(id, content) = blocks[idx] else {
            clearEmoji()
            return
        }
        let token = record.applyingTone(emojiSkinTone) + (trailingSpace ? " " : "")
        let replaceRange = NSRange(
            location: active.colonOffset,
            length: 1 + active.queryLength + active.closingColonLength
        )
        guard replaceRange.location >= 0, NSMaxRange(replaceRange) <= content.length else {
            clearEmoji()
            return
        }
        let mutable = NSMutableAttributedString(attributedString: content)
        mutable.replaceCharacters(
            in: replaceRange,
            with: NSAttributedString(string: token, attributes: MarkdownStyle.baseAttributes)
        )
        blocks[idx] = .text(id: id, attributedContent: mutable)
        bumpRevision(id)
        let caret = active.colonOffset + (token as NSString).length
        desiredSelection = (id, caret)
        // Keep the model's caret in step: the auto-commit path runs INSIDE a
        // text-change recompute, so the text view's next selection callback
        // still carries the pre-commit range.
        selection = (id, NSRange(location: caret, length: 0))
        clearEmoji()
        onEmojiInserted?(record)
        notifyEdit()
    }

    /// The block a caret insertion would land in. Hosts read it BEFORE opening
    /// a picker sheet — presenting one resigns first responder and clears
    /// `focusedBlockId`, so this is what gets re-focused afterwards (EXP-551).
    public var insertionTargetBlockId: UUID? {
        let fallbackId = blocks.last(where: { if case .text = $0 { true } else { false } })?.id
        return focusedBlockId ?? selection?.blockId ?? fallbackId
    }

    /// Insert plain text at the caret of the focused text block (replacing any
    /// selection), falling back to appending to the last text block when no
    /// caret is known — powers the composer's `@` affordance (EXP-240). Follows
    /// the `applyMention` pattern: revision bump + desiredSelection so the text
    /// view re-applies without losing first responder, plus a selection update
    /// so the autocomplete recomputes against the new caret immediately.
    public func insertTextAtCaret(_ text: String) {
        guard let targetId = insertionTargetBlockId,
              let idx = blocks.firstIndex(where: { $0.id == targetId }),
              case let .text(id, content) = blocks[idx] else { return }
        let range: NSRange
        if let sel = selection, sel.blockId == targetId {
            let location = max(0, min(sel.range.location, content.length))
            range = NSRange(location: location, length: min(sel.range.length, content.length - location))
        } else {
            range = NSRange(location: content.length, length: 0)
        }
        let mutable = NSMutableAttributedString(attributedString: content)
        mutable.replaceCharacters(
            in: range,
            with: NSAttributedString(string: text, attributes: MarkdownStyle.baseAttributes)
        )
        blocks[idx] = .text(id: id, attributedContent: mutable)
        bumpRevision(id)
        let caret = range.location + (text as NSString).length
        desiredSelection = (id, caret)
        selection = (id, NSRange(location: caret, length: 0))
        // An explicit `@`/`#` affordance IS a text change, so it may open the
        // bar (EXP-322).
        armTextChange()
        recomputeAutocomplete(armed: true)
        notifyEdit()
    }

    // MARK: - Image insertion

    public func insertImage(data: Data, filename: String, contentType: String, width: Int?, height: Int?) {
        insertImage(
            data: data, filename: filename, contentType: contentType,
            width: width, height: height, atEnd: false
        )
    }

    /// Append an image at the END of the description, ignoring the caret
    /// (EXP-327). An image picked through the *file* picker has no meaningful
    /// insertion point — the user was attaching, not typing — so it lands after
    /// everything already written instead of splitting the focused paragraph.
    public func appendImage(data: Data, filename: String, contentType: String, width: Int?, height: Int?) {
        insertImage(
            data: data, filename: filename, contentType: contentType,
            width: width, height: height, atEnd: true
        )
    }

    private func insertImage(
        data: Data,
        filename: String,
        contentType: String,
        width: Int?,
        height: Int?,
        atEnd: Bool
    ) {
        let draftUrl = MarkdownImageUtils.draftUrl()
        pendingImages[draftUrl] = PendingImage(
            data: data, filename: filename, contentType: contentType, width: width, height: height
        )
        let imageBlockId = UUID()

        let targetId = atEnd ? nil : (focusedBlockId ?? selection?.blockId)
        guard let targetId,
              let blockIndex = blocks.firstIndex(where: { $0.id == targetId }),
              case .text(_, let content) = blocks[blockIndex] else {
            let afterId = UUID()
            blocks.append(.image(id: imageBlockId, url: draftUrl, alt: "image"))
            blocks.append(.text(id: afterId, attributedContent: NSAttributedString()))
            ContentBlock.normalize(&blocks)
            bumpAllRevisions()
            imageUploadStates[imageBlockId] = .idle
            focusedBlockId = afterId
            desiredSelection = (afterId, 0)
            notifyEdit()
            return
        }

        let caret = (selection?.blockId == targetId) ? (selection?.range.location ?? content.length) : content.length
        let cursorPos = max(0, min(caret, content.length))

        let beforeContent: NSAttributedString
        let afterContent: NSAttributedString
        if cursorPos <= 0 {
            beforeContent = NSAttributedString()
            afterContent = content
        } else if cursorPos >= content.length {
            beforeContent = content
            afterContent = NSAttributedString()
        } else {
            beforeContent = content.attributedSubstring(from: NSRange(location: 0, length: cursorPos))
            afterContent = content.attributedSubstring(from: NSRange(location: cursorPos, length: content.length - cursorPos))
        }

        let beforeId = UUID()
        let afterId = UUID()
        blocks.replaceSubrange(blockIndex...blockIndex, with: [
            .text(id: beforeId, attributedContent: beforeContent),
            .image(id: imageBlockId, url: draftUrl, alt: "image"),
            .text(id: afterId, attributedContent: afterContent),
        ])
        bumpRevision(beforeId)
        bumpRevision(afterId)
        imageUploadStates[imageBlockId] = .idle
        focusedBlockId = afterId
        desiredSelection = (afterId, 0)
        selection = (afterId, NSRange(location: 0, length: 0))
        notifyEdit()
    }

    // MARK: - Image deletion / merge

    /// Backspace at the start of a text block deletes the image immediately
    /// above it, merging the surrounding text blocks when both exist.
    public func deleteImage(beforeTextBlock textBlockId: UUID) {
        guard let textIndex = blocks.firstIndex(where: { $0.id == textBlockId }),
              textIndex > 0,
              case .image = blocks[textIndex - 1] else { return }
        dropPendingDraft(at: textIndex - 1)

        if textIndex >= 2,
           case .text(let prevId, let prevContent) = blocks[textIndex - 2],
           case .text(_, let currentContent) = blocks[textIndex] {
            let merged = NSMutableAttributedString(attributedString: prevContent)
            let mergePoint = merged.length
            merged.append(currentContent)
            blocks.replaceSubrange((textIndex - 2)...textIndex, with: [
                .text(id: prevId, attributedContent: merged),
            ])
            bumpRevision(prevId)
            focusedBlockId = prevId
            desiredSelection = (prevId, mergePoint)
        } else {
            blocks.remove(at: textIndex - 1)
            ContentBlock.normalize(&blocks)
            bumpAllRevisions()
        }
        notifyEdit()
    }

    public func deleteImageBlock(id: UUID) {
        guard let index = blocks.firstIndex(where: { $0.id == id }) else { return }
        imageUploadStates[id] = nil
        dropPendingDraft(at: index)

        let prevIndex = index - 1
        let nextIndex = index + 1
        if prevIndex >= 0, nextIndex < blocks.count,
           case .text(let prevId, let prevContent) = blocks[prevIndex],
           case .text(_, let nextContent) = blocks[nextIndex] {
            let merged = NSMutableAttributedString(attributedString: prevContent)
            let mergePoint = merged.length
            merged.append(nextContent)
            blocks.replaceSubrange(prevIndex...nextIndex, with: [
                .text(id: prevId, attributedContent: merged),
            ])
            bumpRevision(prevId)
            focusedBlockId = prevId
            desiredSelection = (prevId, mergePoint)
        } else {
            blocks.remove(at: index)
            ContentBlock.normalize(&blocks)
            bumpAllRevisions()
        }
        notifyEdit()
    }

    // MARK: - Upload commit

    /// Upload all pending draft images concurrently and swap their block URLs to
    /// the returned attachment URLs. Returns `true` only if every referenced
    /// draft resolved (so the caller may persist). On any failure the failed
    /// drafts are kept with a retry state and the description is NOT saved.
    public func commitPendingImages(
        uploader: @escaping @Sendable (PendingImage) async throws -> String
    ) async -> Bool {
        // Remember the uploader so a per-image Retry can re-upload one failed
        // draft directly (a failed block can only exist after a commit ran).
        lastUploader = uploader
        removeDanglingDraftBlocks()

        let drafts: [(blockId: UUID, draftUrl: String, image: PendingImage)] = blocks.compactMap { block in
            guard case .image(let id, let url, _) = block,
                  MarkdownImageUtils.isDraft(url),
                  let image = pendingImages[url] else { return nil }
            return (id, url, image)
        }
        guard !drafts.isEmpty else { return !hasUncommittedDrafts }

        for draft in drafts { imageUploadStates[draft.blockId] = .uploading }

        var results: [(blockId: UUID, draftUrl: String, outcome: UploadOutcome)] = []
        await withTaskGroup(of: (UUID, String, UploadOutcome).self) { group in
            for draft in drafts {
                group.addTask {
                    do {
                        let url = try await uploader(draft.image)
                        return (draft.blockId, draft.draftUrl, .success(url))
                    } catch {
                        return (draft.blockId, draft.draftUrl, .failure(
                            message: error.localizedDescription,
                            reason: uploadFailureReason(error)
                        ))
                    }
                }
            }
            for await result in group {
                results.append((result.0, result.1, result.2))
            }
        }

        var allSucceeded = true
        for entry in results {
            switch entry.outcome {
            case .success(let realUrl):
                setImageURL(blockId: entry.blockId, url: realUrl)
                pendingImages[entry.draftUrl] = nil
                imageUploadStates[entry.blockId] = .idle
            case let .failure(message, reason):
                log.error("Image upload failed: \(message, privacy: .public)")
                imageUploadStates[entry.blockId] = .failed(reason)
                allSucceeded = false
            }
        }
        return allSucceeded && !hasUncommittedDrafts
    }

    /// Sendable upload result used to cross the `withTaskGroup` boundary
    /// (a `Result<String, any Error>` would not be `Sendable`).
    private enum UploadOutcome: Sendable {
        case success(String)
        case failure(message: String, reason: ImageUploadFailureReason)
    }

    private var lastUploader: (@Sendable (PendingImage) async throws -> String)?

    /// Re-upload ONE failed draft image (the block's Retry button), using the
    /// uploader remembered from the last commit. On success the block swaps to
    /// the real attachment URL and the host's edit hook fires so it can re-save.
    public func retryImage(blockId: UUID) async {
        guard let uploader = lastUploader,
              case .failed = uploadState(for: blockId),
              let idx = blocks.firstIndex(where: { $0.id == blockId }),
              case .image(_, let draftUrl, _) = blocks[idx],
              MarkdownImageUtils.isDraft(draftUrl),
              let image = pendingImages[draftUrl] else { return }
        imageUploadStates[blockId] = .uploading
        do {
            let realUrl = try await uploader(image)
            setImageURL(blockId: blockId, url: realUrl)
            pendingImages[draftUrl] = nil
            imageUploadStates[blockId] = .idle
            notifyEdit()
        } catch {
            log.error("Image retry failed: \(error.localizedDescription, privacy: .public)")
            imageUploadStates[blockId] = .failed(uploadFailureReason(error))
        }
    }

    // MARK: - Internals

    private func setImageURL(blockId: UUID, url: String) {
        guard let idx = blocks.firstIndex(where: { $0.id == blockId }),
              case .image(let id, _, let alt) = blocks[idx] else { return }
        // Changing the URL re-runs BlockImageView's load task against the real
        // attachment; the loader cache is pre-seeded so it does not re-download.
        blocks[idx] = .image(id: id, url: url, alt: alt)
    }

    private func dropPendingDraft(at index: Int) {
        guard index >= 0, index < blocks.count,
              case .image(_, let url, _) = blocks[index],
              MarkdownImageUtils.isDraft(url) else { return }
        pendingImages[url] = nil
    }

    /// Remove `draft://` image blocks that have no backing pending data (e.g.
    /// the in-memory bytes were lost to an app restart before commit).
    private func removeDanglingDraftBlocks() {
        var changed = false
        for index in stride(from: blocks.count - 1, through: 0, by: -1) {
            if case .image(let id, let url, _) = blocks[index],
               MarkdownImageUtils.isDraft(url),
               pendingImages[url] == nil {
                blocks.remove(at: index)
                imageUploadStates[id] = nil
                changed = true
            }
        }
        if changed {
            ContentBlock.normalize(&blocks)
            bumpAllRevisions()
        }
    }

    private func bumpRevision(_ id: UUID) {
        revisionCounter += 1
        revisions[id] = revisionCounter
    }

    private func bumpAllRevisions() {
        for block in blocks {
            revisionCounter += 1
            revisions[block.id] = revisionCounter
        }
    }

    private func notifyEdit() {
        onEdit?()
    }
}
