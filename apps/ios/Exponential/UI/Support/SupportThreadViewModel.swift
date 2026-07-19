import ExpCore
import Foundation
import GRDB

/// One support ticket's conversation (EXP-180). tRPC-polled at 15s (tickets
/// are not Electric-synced); reply/note/close/reopen/escalate mutate then
/// refresh immediately so the transcript reflects the action without waiting
/// for the next poll tick.
@MainActor @Observable
final class SupportThreadViewModel {
    var thread: SupportThreadInfo?
    var messages: [SupportMessage] = []
    var linkedIssue: SupportLinkedIssue?
    /// Non-archived boards of the ticket's team (GRDB, loaded with the
    /// thread) — the escalation picker's options.
    var boards: [BoardEntity] = []
    var isLoading = false
    var error: String?
    var sending = false

    private let accountId: String
    private let threadId: String
    private let helpdeskApi: HelpdeskApi
    private let db: DatabaseManager
    private var pollTask: Task<Void, Never>?

    init(accountId: String, threadId: String, helpdeskApi: HelpdeskApi, db: DatabaseManager) {
        self.accountId = accountId
        self.threadId = threadId
        self.helpdeskApi = helpdeskApi
        self.db = db
    }

    var isOpen: Bool { thread?.status == "open" }

    func startPolling() {
        stopPolling()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.load()
                try? await Task.sleep(for: .seconds(15))
            }
        }
    }

    func stopPolling() {
        pollTask?.cancel()
        pollTask = nil
    }

    func load() async {
        if thread == nil { isLoading = true }
        defer { isLoading = false }
        do {
            let detail = try await helpdeskApi.getThread(accountId: accountId, threadId: threadId)
            thread = detail.thread
            messages = detail.messages
            linkedIssue = detail.linkedIssue
            loadBoards(teamId: detail.thread.teamId)
            error = nil
        } catch is CancellationError {
            // Expected on stopPolling.
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Send the composer text as a public reply or an internal note.
    /// Returns true when it went through (the view clears the field then).
    func send(body: String, internalNote: Bool) async -> Bool {
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !sending else { return false }
        sending = true
        defer { sending = false }
        do {
            if internalNote {
                try await helpdeskApi.note(accountId: accountId, threadId: threadId, body: trimmed)
            } else {
                try await helpdeskApi.reply(accountId: accountId, threadId: threadId, body: trimmed)
            }
            await load()
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    func close() async {
        do {
            try await helpdeskApi.close(accountId: accountId, threadId: threadId)
            await load()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func reopen() async {
        do {
            try await helpdeskApi.reopen(accountId: accountId, threadId: threadId)
            await load()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func escalate(boardId: String) async {
        do {
            _ = try await helpdeskApi.escalate(
                accountId: accountId, threadId: threadId, boardId: boardId
            )
            await load()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func loadBoards(teamId: String) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let rows = (try? pool.read { db in try BoardEntity.fetchAll(db) }) ?? []
        boards = rows
            .filter { $0.teamId == teamId && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }
}
