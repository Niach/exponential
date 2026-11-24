import Foundation

/// #NEW-136: the model behind `Translate to a search`. On iOS it is Apple
/// Foundation Models on the device (the app's `FoundationModelsSearchTranslator`);
/// tests and captures hand in a fake.
public protocol IssueSearchTranslating: Sendable {
    /// Whether the model can answer right now (`false` = the search offers
    /// no row).
    var isAvailable: Bool { get }
    /// ONE answer to `prompt` under `instructions`.
    func answer(prompt: String, instructions: String) async throws -> String
}

/// #NEW-136: one translate call from the typed text to a query the engine
/// accepts: the shared prompt (`IssueSearchTranslation.prompt`), the model,
/// the hard deadline, the sanitizer. The model's answer is never trusted and
/// never waited for past `timeout`.
public enum IssueSearchTranslator {
    /// Resumes its continuation once, whoever comes first: the answer, the
    /// deadline or a cancel.
    private final class Once: @unchecked Sendable {
        private let lock = NSLock()
        private var continuation: CheckedContinuation<String?, Never>?
        private var settled = false

        /// False when the race is already over (the continuation resumes nil).
        func arm(_ continuation: CheckedContinuation<String?, Never>) {
            lock.lock()
            if settled {
                lock.unlock()
                continuation.resume(returning: nil)
                return
            }
            self.continuation = continuation
            lock.unlock()
        }

        func settle(_ value: String?) {
            lock.lock()
            let waiting = continuation
            continuation = nil
            settled = true
            lock.unlock()
            waiting?.resume(returning: value)
        }
    }

    /// The query `text` means according to `translator`, or nil: the model
    /// failed, ran past `timeout`, the task was cancelled, or the sanitizer
    /// kept nothing (`IssueSearchTranslation.sanitize`).
    public static func translate(
        _ text: String,
        members: [IssueSearchPhrase.Member],
        now: Date = Date(),
        translator: any IssueSearchTranslating,
        timeout: Duration = .seconds(IssueSearchTranslation.Limits.timeoutSeconds)
    ) async -> String? {
        let prompt = IssueSearchTranslation.prompt(text, members: members, now: now)
        let once = Once()
        let work = Task<Void, Never> {
            once.settle(try? await translator.answer(prompt: prompt, instructions: IssueSearchTranslation.system))
        }
        let clock = Task<Void, Never> {
            try? await Task.sleep(for: timeout)
            once.settle(nil)
        }
        // A model that ignores cancellation cannot hold the caller: the
        // race answers at the deadline or the cancel, the work is let go.
        let answer = await withTaskCancellationHandler {
            await withCheckedContinuation { once.arm($0) }
        } onCancel: {
            once.settle(nil)
        }
        work.cancel()
        clock.cancel()
        guard let answer, !Task.isCancelled else { return nil }
        return IssueSearchTranslation.sanitize(answer, text: text, members: members, now: now)
    }
}
