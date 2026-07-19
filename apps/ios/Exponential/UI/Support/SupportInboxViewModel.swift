import ExpCore
import Foundation

/// Support inbox (EXP-180): standalone helpdesk tickets, polled over tRPC —
/// they are deliberately NOT an Electric shape. A 30s poll loop (the
/// IssueDetailViewModel liveness-clock precedent) keeps the list fresh while
/// the segment is on screen; the view re-arms it on every appear.
@MainActor @Observable
final class SupportInboxViewModel {
    var threads: [SupportThreadRow] = []
    var filter: SupportThreadFilter = .open
    var isLoading = false
    var error: String?

    private let helpdeskApi: HelpdeskApi
    private var accountId: String?
    private var teamId: String?
    private var pollTask: Task<Void, Never>?
    /// Bumped on every (re)start/filter flip so a stale in-flight response
    /// can't clobber the fresh context's rows.
    private var generation = 0

    init(helpdeskApi: HelpdeskApi) {
        self.helpdeskApi = helpdeskApi
    }

    func startPolling(accountId: String, teamId: String) {
        stopPolling()
        if self.accountId != accountId || self.teamId != teamId {
            // New team/account context — drop the previous team's rows.
            threads = []
            error = nil
        }
        self.accountId = accountId
        self.teamId = teamId
        generation += 1
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.load()
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    func stopPolling() {
        pollTask?.cancel()
        pollTask = nil
    }

    func setFilter(_ newFilter: SupportThreadFilter) {
        guard newFilter != filter else { return }
        filter = newFilter
        threads = []
        error = nil
        generation += 1
        // Immediate reload; the running poll loop keeps its cadence.
        Task { await load() }
    }

    func load() async {
        guard let accountId, let teamId else { return }
        let requestGeneration = generation
        let requestFilter = filter
        if threads.isEmpty { isLoading = true }
        defer { if requestGeneration == generation { isLoading = false } }
        do {
            let rows = try await helpdeskApi.listThreads(
                accountId: accountId, teamId: teamId, filter: requestFilter
            )
            guard requestGeneration == generation else { return }
            threads = rows
            error = nil
        } catch is CancellationError {
            // Expected on stopPolling.
        } catch {
            guard requestGeneration == generation else { return }
            self.error = error.localizedDescription
        }
    }
}
