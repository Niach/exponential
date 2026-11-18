import Foundation

/// #EDE-114 THE PHONE WALK (chat-store.json `catchUp`): the facade's `CatchUp`
/// carried over eNet. `concurrency()` workers each loop
/// `nextWalk` → (`nextAsk` → wait the bucket → send → await the
/// `history_chunk` naming that requestId, at most `askTimeoutMs` →
/// `takeAnswer`) until the pass has nothing left. Chunks reach it through
/// `deliver` (the node's inbox hands them over before any viewer link sees
/// them). `stop` ends a pass where it is: the store's watermarks keep the place.
public final class ChatCatchUpLoop: @unchecked Sendable {
    /// Carries one ask: `frame` in a steer envelope of `sessionId` to `peer`.
    public typealias Send = @Sendable (_ peer: String, _ sessionId: String, _ frame: String) async throws -> Void

    private final class Waiter {
        var value: String??
        var continuation: CheckedContinuation<String?, Never>?
    }

    public let walker: ChatCatchUpWalker
    private let send: Send
    private let now: @Sendable () -> Int64
    private let lock = NSLock()
    private var waiters: [String: Waiter] = [:]
    private var stopped = false
    private var running: Task<UInt32, Never>?

    public init(walker: ChatCatchUpWalker, now: @escaping @Sendable () -> Int64 = ChatCatchUpLoop.nowMs, send: @escaping Send) {
        self.walker = walker
        self.send = send
        self.now = now
    }

    public static let nowMs: @Sendable () -> Int64 = { Int64((Date().timeIntervalSince1970 * 1000).rounded()) }

    /// Whether `requestId` is an ask of this loop still waiting for its chunk.
    public func isPending(_ requestId: String) -> Bool {
        lock.withLock { waiters[requestId] != nil }
    }

    /// A `history_chunk` arrived: true when it answers one of this loop's
    /// pending asks (it is then this loop's, never a viewer's).
    @discardableResult
    public func deliver(requestId: String, chunk: String?) -> Bool {
        let (found, continuation): (Bool, CheckedContinuation<String?, Never>?) = lock.withLock {
            guard let waiter = waiters[requestId] else { return (false, nil) }
            if let continuation = waiter.continuation {
                waiters.removeValue(forKey: requestId)
                return (true, continuation)
            }
            if waiter.value == nil { waiter.value = .some(chunk) }
            return (true, nil)
        }
        continuation?.resume(returning: chunk)
        return found
    }

    /// One pass over `sessions` (nil back = a pass is already running).
    /// Returns how many events it stored.
    @discardableResult
    public func runPass(_ sessions: [ChatCatchUpSession]) async -> UInt32? {
        let task: Task<UInt32, Never>? = lock.withLock {
            guard running == nil else { return nil }
            stopped = false
            let task = Task.detached(priority: .utility) { [self] in await self.pass(sessions) }
            running = task
            return task
        }
        guard let task else { return nil }
        let added = await task.value
        lock.withLock { running = nil }
        return added
    }

    public var isRunning: Bool { lock.withLock { running != nil } }

    /// Ends the pass where it is (the OS ended the window, the account left).
    public func stop() {
        let (pending, task): ([CheckedContinuation<String?, Never>], Task<UInt32, Never>?) = lock.withLock {
            stopped = true
            let out = waiters.values.compactMap(\.continuation)
            waiters.removeAll()
            return (out, running)
        }
        walker.stop()
        task?.cancel()
        for continuation in pending { continuation.resume(returning: nil) }
    }

    private var isStopped: Bool { lock.withLock { stopped } }

    private func pass(_ sessions: [ChatCatchUpSession]) async -> UInt32 {
        guard walker.beginPass(sessions: sessions, nowMs: now()) > 0 else { return 0 }
        let workers = max(1, walker.concurrency)
        return await withTaskGroup(of: UInt32.self) { group in
            for _ in 0..<workers {
                group.addTask { await self.worker() }
            }
            var added: UInt32 = 0
            for await count in group { added &+= count }
            return added
        }
    }

    private func worker() async -> UInt32 {
        var added: UInt32 = 0
        while !isStopped, !Task.isCancelled, let sessionId = walker.nextWalk(nowMs: now()) {
            while !isStopped, !Task.isCancelled, let ask = walker.nextAsk(sessionId: sessionId, nowMs: now()) {
                if ask.waitMs > 0 { try? await Task.sleep(for: .milliseconds(Int(min(ask.waitMs, 60_000)))) }
                let chunk = await carry(ask)
                guard !isStopped else { return added }
                if let taken = walker.takeAnswer(requestId: ask.requestId, chunkJson: chunk, nowMs: now()) {
                    added &+= taken.added
                }
            }
        }
        return added
    }

    /// Sends one ask and waits for its chunk (nil = unreachable or no
    /// answer within `askTimeoutMs`).
    private func carry(_ ask: ChatCatchUpAsk) async -> String? {
        guard lock.withLock({ () -> Bool in
            guard !stopped else { return false }
            waiters[ask.requestId] = Waiter()
            return true
        }) else { return nil }
        do {
            try await send(ask.peer, ask.sessionId, ask.frame)
        } catch {
            deliver(requestId: ask.requestId, chunk: nil)
        }
        let timeout = Task.detached { [weak self, walker] in
            try? await Task.sleep(for: .milliseconds(Int(walker.askTimeoutMs)))
            guard !Task.isCancelled else { return }
            self?.deliver(requestId: ask.requestId, chunk: nil)
        }
        defer { timeout.cancel() }
        return await withCheckedContinuation { continuation in
            let ready: String?? = lock.withLock {
                guard let waiter = waiters[ask.requestId] else { return .some(nil) }
                if let value = waiter.value {
                    waiters.removeValue(forKey: ask.requestId)
                    return .some(value)
                }
                waiter.continuation = continuation
                return nil
            }
            if let ready { continuation.resume(returning: ready) }
        }
    }
}
