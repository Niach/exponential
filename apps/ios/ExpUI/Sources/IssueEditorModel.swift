import Foundation
import os

private let log = Logger(subsystem: "com.exponential", category: "IssueEditorModel")

/// Upload state for an inline image block, surfaced as status / retry UI.
public enum ImageUploadState: Equatable, Sendable {
    case idle
    case uploading
    case failed
}

/// A workspace member the @-autocomplete can offer. The canonical interchange
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

    /// Workspace members the @-autocomplete can offer (set by the host; should be
    /// pre-filtered to non-agent members).
    public var mentionMembers: [MentionMember] = []

    /// Render-only resolver for inline `#IDENTIFIER` issue refs: identifier
    /// (e.g. `VER-12`) → issue id, from the host's local store. When set,
    /// `load()` decorates resolved tokens as tappable pills (unknown
    /// identifiers stay plain text). Purely display — the derived markdown is
    /// byte-identical either way.
    public var issueRefResolver: ((String) -> String?)?

    /// Active @-mention candidates for the focused block's caret query, recomputed
    /// on edit/selection. Empty when no mention token is being typed.
    public private(set) var mentionCandidates: [MentionMember] = []

    // The @-token currently being edited: where the `@` is and how long the query
    // after it is, in the focused block.
    private var activeMention: (blockId: UUID, atOffset: Int, queryLength: Int)?

    // An @mention at the caret: `@` preceded by start-of-text or whitespace, then
    // an email-ish run. Mirrors the web/Android regex.
    private static let mentionRegex = try! NSRegularExpression(
        pattern: "(?:^|\\s)@([A-Za-z0-9._%+-]*)$"
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

    public func load(markdown: String, baseURL: URL?) {
        blocks = MarkdownConversion.markdownToBlocks(markdown, baseURL: baseURL)
        decorateIssueRefs()
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

    /// Decorate resolved `#IDENTIFIER` tokens in every text block (render-only;
    /// see `issueRefResolver`). Called from `load()` so remote applies and
    /// user-driven reloads re-decorate too.
    private func decorateIssueRefs() {
        guard let issueRefResolver else { return }
        for (idx, block) in blocks.enumerated() {
            guard case let .text(id, content) = block else { continue }
            let decorated = IssueRefs.decorate(content, resolver: issueRefResolver)
            if decorated !== content {
                blocks[idx] = .text(id: id, attributedContent: decorated)
            }
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
        recomputeMention()
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
        recomputeMention()
        notifyEdit()
    }

    // MARK: - Mentions

    /// Recompute the @-mention candidates from the focused block's caret context.
    /// Driven off the model's own `selection` + `blocks`, so no caret geometry is
    /// needed in the text views.
    private func recomputeMention() {
        guard !mentionMembers.isEmpty,
              let sel = selection,
              let block = blocks.first(where: { $0.id == sel.blockId }),
              case let .text(_, content) = block else {
            clearMention()
            return
        }
        let caret = max(0, min(sel.range.location, content.length))
        let before = (content.string as NSString).substring(to: caret)
        guard let match = Self.mentionMatch(beforeCaret: before) else {
            clearMention()
            return
        }
        activeMention = (sel.blockId, match.atOffset, match.query.count)
        let q = match.query.lowercased()
        mentionCandidates = mentionMembers
            .filter { q.isEmpty || $0.name.lowercased().contains(q) || $0.email.lowercased().contains(q) }
            .prefix(6)
            .map { $0 }
    }

    private func clearMention() {
        activeMention = nil
        if !mentionCandidates.isEmpty { mentionCandidates = [] }
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

    // MARK: - Image insertion

    public func insertImage(data: Data, filename: String, contentType: String, width: Int?, height: Int?) {
        let draftUrl = MarkdownImageUtils.draftUrl()
        pendingImages[draftUrl] = PendingImage(
            data: data, filename: filename, contentType: contentType, width: width, height: height
        )
        let imageBlockId = UUID()

        let targetId = focusedBlockId ?? selection?.blockId
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
                        return (draft.blockId, draft.draftUrl, .failure(error.localizedDescription))
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
            case .failure(let message):
                log.error("Image upload failed: \(message, privacy: .public)")
                imageUploadStates[entry.blockId] = .failed
                allSucceeded = false
            }
        }
        return allSucceeded && !hasUncommittedDrafts
    }

    /// Sendable upload result used to cross the `withTaskGroup` boundary
    /// (a `Result<String, any Error>` would not be `Sendable`).
    private enum UploadOutcome: Sendable {
        case success(String)
        case failure(String)
    }

    private var lastUploader: (@Sendable (PendingImage) async throws -> String)?

    /// Re-upload ONE failed draft image (the block's Retry button), using the
    /// uploader remembered from the last commit. On success the block swaps to
    /// the real attachment URL and the host's edit hook fires so it can re-save.
    public func retryImage(blockId: UUID) async {
        guard let uploader = lastUploader,
              uploadState(for: blockId) == .failed,
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
            imageUploadStates[blockId] = .failed
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
