import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #FEED-219 (fixtures/composer-draft.json): every composer's field is
// mirrored into a synced `composer_drafts` row keyed by its target, so a tab
// switch, a navigation, a relaunch or another device shows the text again.
// `ComposerDrafts` owns one `ComposerDraftWriter` per account (debounced,
// flushed when a composer goes away and when the app backgrounds); a landed
// write is mirrored into the local store at once, so a remount reads it
// before the long-poll echoes it. `.composerDraft(...)` binds one field.

@MainActor
final class ComposerDrafts {
    private let db: DatabaseManager
    private let api: ComposerDraftsApi
    private var writers: [String: ComposerDraftWriter] = [:]

    nonisolated init(db: DatabaseManager, api: ComposerDraftsApi) {
        self.db = db
        self.api = api
    }

    func writer(accountId: String) -> ComposerDraftWriter {
        if let writer = writers[accountId] { return writer }
        let db = db
        let api = api
        let writer = ComposerDraftWriter(transport: .init(
            upsert: { teamId, key, text in
                let row = try await api.upsert(accountId: accountId, teamId: teamId, key: key, text: text)
                let pool = try db.pool(forAccountId: accountId)
                try await pool.write { db in
                    try ComposerDraftEntity.filter(Column("key") == key).deleteAll(db)
                    try row.entity().save(db)
                }
            },
            delete: { key in
                try await api.delete(accountId: accountId, key: key)
                let pool = try db.pool(forAccountId: accountId)
                _ = try await pool.write { db in
                    try ComposerDraftEntity.filter(Column("key") == key).deleteAll(db)
                }
            }
        ))
        writers[accountId] = writer
        return writer
    }

    /// A send settled: write what the field it left keeps, now (composer-send
    /// first, then this). For a host whose send also switches the target.
    func settle(_ target: ComposerDraft.Target, teamId: String?, accountId: String, text: String) {
        guard let teamId else { return }
        writer(accountId: accountId).write(key: target.key, teamId: teamId, text: text, now: true)
    }

    /// The app is backgrounding: every pending draft goes out now.
    func flushAll() {
        for writer in writers.values { writer.flushAll() }
    }

    /// The synced text of one key, live (nil = no row).
    func synced(accountId: String, key: String) -> AsyncValueObservation<String?>? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return ValueObservation
            .tracking { db in
                try String.fetchOne(
                    db,
                    sql: #"SELECT "text" FROM "composer_drafts" WHERE "key" = ? LIMIT 1"#,
                    arguments: [key]
                )
            }
            .removeDuplicates()
            .values(in: pool)
    }
}

extension View {
    /// Back one composer field with the synced draft of `target`. `revision`
    /// changes on every edit of the field (its text, or a counter when
    /// reading the text is costly); `read` returns the field's text and
    /// `apply` replaces it. `target` or `teamId` nil = a plain field (an
    /// edit composer, no team yet).
    func composerDraft<Revision: Equatable>(
        _ target: ComposerDraft.Target?,
        teamId: String?,
        revision: Revision,
        read: @escaping () -> String,
        apply: @escaping (String) -> Void
    ) -> some View {
        modifier(ComposerDraftModifier(
            key: teamId == nil ? nil : target?.key,
            teamId: teamId,
            revision: revision,
            read: read,
            apply: apply
        ))
    }
}

private struct Stamp<Revision: Equatable>: Equatable {
    let key: String?
    let revision: Revision
}

private struct ComposerDraftModifier<Revision: Equatable>: ViewModifier {
    let key: String?
    let teamId: String?
    let revision: Revision
    let read: () -> String
    let apply: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    /// The person edited the field since the composer opened (on this key).
    @State private var edited = false
    /// The text this modifier last put into the field: its own echo is not
    /// an edit, and only a field still showing a draft follows the row away.
    @State private var shown: String?
    /// The key the field was last bound to (a switch flushes the old one).
    @State private var boundKey: String?

    func body(content: Content) -> some View {
        content
            .task(id: "\(accountId)|\(key ?? "")") { await follow() }
            // An edit landing together with a target switch belongs to the
            // target left (its host writes it, `ComposerDrafts.settle`).
            .onChange(of: Stamp(key: key, revision: revision)) { old, new in
                guard old.key == new.key else { return }
                fieldChanged()
            }
            .onDisappear {
                guard let key else { return }
                let writer = deps.composerDrafts.writer(accountId: accountId)
                Task { await writer.flush(key) }
            }
    }

    private func follow() async {
        let writer = deps.composerDrafts.writer(accountId: accountId)
        let switched = boundKey != nil && boundKey != key
        if let old = boundKey, old != key {
            Task { await writer.flush(old) }
        }
        boundKey = key
        edited = false
        guard let key else { return }
        // A switch to another target empties the field it leaves; a fresh
        // mount never wipes what a host seeded.
        shown = switched ? read() : nil
        if let local = writer.local(key), switched || read().isEmpty { show(local) }
        guard let values = deps.composerDrafts.synced(accountId: accountId, key: key) else { return }
        do {
            for try await synced in values {
                guard !edited else { continue }
                let next = ComposerDraft.field(.init(
                    field: read(), edited: false, local: writer.local(key), synced: synced
                ))
                // A fresh mount never overwrites what its host put there (a
                // seed, a model that outlived the screen), nor clears it.
                if shown == nil, next.isEmpty || !read().isEmpty { continue }
                show(next)
            }
        } catch {}
    }

    private func show(_ text: String) {
        if read() != text { apply(text) }
        // The field's own reading of it (a markdown editor normalises).
        shown = read()
    }

    private func fieldChanged() {
        guard let key, let teamId else { return }
        let text = read()
        if !edited, text == shown { return }
        edited = true
        shown = nil
        // An emptied field (a send that cleared it, a discard) deletes at once.
        let now = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        deps.composerDrafts.writer(accountId: accountId)
            .write(key: key, teamId: teamId, text: text, now: now)
    }
}
