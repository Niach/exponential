import Foundation

/// #FEED-219 (fixtures/composer-draft.json `writes`): the writer every
/// composer's draft goes through, one per account. A field's text is written
/// `ComposerDraft.debounceMs` after the last keystroke, at once when the
/// composer goes away, after a send settles and when the app backgrounds.
/// Until a write has landed, its text stays here as the key's LOCAL copy (the
/// fixture's `local`), so a composer remounting in between never shows an
/// older synced row. A failed write is silent: the copy stays and the next
/// write retries.
@MainActor
public final class ComposerDraftWriter {
    public struct Transport: Sendable {
        public let upsert: @Sendable (_ teamId: String, _ key: String, _ text: String) async throws -> Void
        public let delete: @Sendable (_ key: String) async throws -> Void

        public init(
            upsert: @escaping @Sendable (_ teamId: String, _ key: String, _ text: String) async throws -> Void,
            delete: @escaping @Sendable (_ key: String) async throws -> Void
        ) {
            self.upsert = upsert
            self.delete = delete
        }
    }

    private final class Entry {
        var teamId: String
        /// The newest text for the key, written or not.
        var text: String
        var timer: Task<Void, Never>?
        /// A write is in flight; `dirty` = another one is due after it.
        var writing = false
        var dirty = false

        init(teamId: String, text: String) {
            self.teamId = teamId
            self.text = text
        }
    }

    private let transport: Transport
    private let debounce: Duration
    private var entries: [String: Entry] = [:]

    public init(transport: Transport, debounce: Duration = .milliseconds(ComposerDraft.debounceMs)) {
        self.transport = transport
        self.debounce = debounce
    }

    /// The key's own write not yet landed (nil = nothing pending).
    public func local(_ key: String) -> String? {
        entries[key]?.text
    }

    /// Record the field's text; it is written after the debounce, or at once.
    public func write(key: String, teamId: String, text: String, now: Bool = false) {
        let entry = entries[key] ?? Entry(teamId: teamId, text: text)
        entries[key] = entry
        entry.teamId = teamId
        entry.text = text
        entry.dirty = true
        entry.timer?.cancel()
        entry.timer = nil
        if now {
            Task { await flush(key) }
            return
        }
        let debounce = debounce
        entry.timer = Task { [weak self] in
            try? await Task.sleep(for: debounce)
            guard !Task.isCancelled else { return }
            await self?.flush(key)
        }
    }

    /// Write the key's pending text now (no-op when nothing is due).
    public func flush(_ key: String) async {
        guard let entry = entries[key] else { return }
        entry.timer?.cancel()
        entry.timer = nil
        guard !entry.writing, entry.dirty else { return }
        let text = entry.text
        entry.dirty = false
        entry.writing = true
        do {
            switch ComposerDraft.write(text) {
            case let .upsert(value): try await transport.upsert(entry.teamId, key, value)
            case .delete: try await transport.delete(key)
            }
        } catch {
            // Silent: the local copy stays, the next write retries.
            entry.dirty = true
        }
        entry.writing = false
        guard entries[key] === entry else { return }
        if entry.dirty, entry.text != text {
            // Typed while the write was in flight: write the newer text.
            if entry.timer == nil { await flush(key) }
            return
        }
        if !entry.dirty, entry.timer == nil { entries[key] = nil }
    }

    /// Write every pending draft now (the app is backgrounding).
    public func flushAll() {
        for key in entries.keys {
            Task { await flush(key) }
        }
    }
}
