import Foundation
import os
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "IssueEditorModel")

/// Upload state for an inline image block, surfaced as status / retry UI.
enum ImageUploadState: Equatable, Sendable {
    case idle
    case uploading
    case failed
}

/// Single source of truth for the block-based markdown editor. The editor view
/// renders `blocks` and routes every edit through this model; markdown is
/// *derived* (`currentMarkdown()`) only at save points, never round-tripped on
/// each keystroke. This eliminates the old `@State blocks` + `@Binding text`
/// race and the per-keystroke cmark re-serialization.
@MainActor
@Observable
final class IssueEditorModel {
    /// The document, as ordered text/image blocks. Mutated only through the
    /// intent methods below so revisions and pending state stay consistent.
    private(set) var blocks: [ContentBlock] = []

    /// In-memory image data keyed by its `draft://` placeholder URL, kept until
    /// the image is uploaded and its block URL swapped to the real attachment.
    var pendingImages: [String: PendingImage] = [:]

    /// Currently focused text block, drives first-responder + autosave-on-blur.
    private(set) var focusedBlockId: UUID?

    /// A remote (Electric) update that arrived while the user was actively
    /// editing or had unsaved edits — surfaced as a non-blocking reload banner.
    private(set) var pendingRemoteMarkdown: String?

    /// Markdown last persisted to the server, used to detect local dirtiness.
    private(set) var lastSavedMarkdown: String = ""

    /// Per-image upload state for inline status / retry affordances.
    private(set) var imageUploadStates: [UUID: ImageUploadState] = [:]

    /// Caret the editor should move to after the next structural mutation.
    private(set) var desiredSelection: (blockId: UUID, location: Int)?

    /// Invoked on user-originated edits so the host can schedule a debounced save.
    var onEdit: (() -> Void)?

    // Monotonic content revisions, bumped ONLY for external/structural changes
    // (load, remote apply, insert, delete, merge) — never for the user's own
    // keystrokes — so a text view never clobbers what the user just typed.
    private var revisions: [UUID: Int] = [:]
    private var revisionCounter = 0

    // Last known caret in the focused text block, used for image insertion.
    private var selection: (blockId: UUID, range: NSRange)?

    init() {
        // Always start with one empty text block so a fresh editor (e.g. the
        // create sheet, before any load) is immediately typeable.
        let id = UUID()
        blocks = [.text(id: id, attributedContent: NSAttributedString())]
        revisions[id] = 0
    }

    // MARK: - Derived state

    func currentMarkdown() -> String {
        MarkdownConversion.blocksToMarkdown(blocks)
    }

    var isEditing: Bool { focusedBlockId != nil }
    var isDirty: Bool { currentMarkdown() != lastSavedMarkdown }
    var hasUncommittedDrafts: Bool { MarkdownImageUtils.hasDraftImages(currentMarkdown()) }

    func revision(for id: UUID) -> Int { revisions[id] ?? 0 }
    func uploadState(for id: UUID) -> ImageUploadState { imageUploadStates[id] ?? .idle }

    // MARK: - Loading / reconciliation

    func load(markdown: String, baseURL: URL?) {
        blocks = MarkdownConversion.markdownToBlocks(markdown, baseURL: baseURL)
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
    func applyRemote(markdown: String, baseURL: URL?) {
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

    func reloadPendingRemote(baseURL: URL?) {
        guard let pending = pendingRemoteMarkdown else { return }
        load(markdown: pending, baseURL: baseURL)
    }

    func markSaved(_ markdown: String) {
        lastSavedMarkdown = markdown
        if pendingRemoteMarkdown == markdown { pendingRemoteMarkdown = nil }
    }

    // MARK: - Focus / selection (reported by the text views)

    func setFocused(_ id: UUID?) { focusedBlockId = id }

    func clearFocusIfMatches(_ id: UUID) {
        if focusedBlockId == id { focusedBlockId = nil }
    }

    func updateSelection(blockId: UUID, range: NSRange) {
        selection = (blockId, range)
    }

    /// The post-mutation caret location for `id`, consumed once.
    func consumeDesiredSelection(for id: UUID) -> Int? {
        guard desiredSelection?.blockId == id else { return nil }
        let location = desiredSelection?.location
        desiredSelection = nil
        return location
    }

    // MARK: - Text editing

    func updateText(id: UUID, content: NSAttributedString) {
        guard let idx = blocks.firstIndex(where: { $0.id == id }) else { return }
        // No revision bump: the originating text view already holds this content.
        blocks[idx] = .text(id: id, attributedContent: content)
        notifyEdit()
    }

    // MARK: - Image insertion

    func insertImage(data: Data, filename: String, contentType: String, width: Int?, height: Int?) {
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
    func deleteImage(beforeTextBlock textBlockId: UUID) {
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

    func deleteImageBlock(id: UUID) {
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
    func commitPendingImages(
        uploader: @escaping @Sendable (PendingImage) async throws -> String
    ) async -> Bool {
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
