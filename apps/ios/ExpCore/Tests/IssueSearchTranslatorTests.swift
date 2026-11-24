import Foundation
import XCTest
@testable import ExpCore

// #NEW-136: the translate call's glue with a fake model: the shared prompt
// goes in, the sanitizer decides what comes out, and neither a failing nor
// a hanging model holds the search.
final class IssueSearchTranslatorTests: XCTestCase {
    private final class Fake: IssueSearchTranslating, @unchecked Sendable {
        enum Behaviour {
            case answer(String)
            case fail
            /// Never answers and ignores cancellation.
            case hang
        }

        let isAvailable = true
        private let behaviour: Behaviour
        private let lock = NSLock()
        private var seen: [(prompt: String, instructions: String)] = []

        init(_ behaviour: Behaviour) { self.behaviour = behaviour }

        var calls: [(prompt: String, instructions: String)] {
            lock.lock()
            defer { lock.unlock() }
            return seen
        }

        func answer(prompt: String, instructions: String) async throws -> String {
            lock.withLock { seen.append((prompt, instructions)) }
            switch behaviour {
            case let .answer(text): return text
            case .fail: throw CancellationError()
            case .hang:
                await withUnsafeContinuation { (_: UnsafeContinuation<Void, Never>) in }
                return ""
            }
        }
    }

    private let members = [IssueSearchPhrase.Member(name: "Anna Berg", email: "anna@acme.test")]
    private let now = Date(timeIntervalSince1970: 1_794_571_200) // 2026-11-13T12:00:00Z
    private let text = "what is still broken in the editor toolbar"

    func testAnAnswerBecomesTheSanitizedQuery() async {
        let fake = Fake(.answer("```\nSearch: is:open by:anna label:bug editor toolbar\n```"))
        let query = await IssueSearchTranslator.translate(text, members: members, now: now, translator: fake)
        XCTAssertEqual(query, "is:open by:anna editor toolbar")
        // ONE call, with the shared prompt and the shared instruction line.
        XCTAssertEqual(fake.calls.count, 1)
        XCTAssertEqual(fake.calls.first?.prompt, IssueSearchTranslation.prompt(text, members: members, now: now))
        XCTAssertEqual(fake.calls.first?.instructions, IssueSearchTranslation.system)
    }

    func testGarbageIsNoQuery() async {
        let fake = Fake(.answer("label:bug priority:high !!!"))
        let query = await IssueSearchTranslator.translate(text, members: members, now: now, translator: fake)
        XCTAssertNil(query)
    }

    func testTheTextEchoedBackIsNoQuery() async {
        let fake = Fake(.answer(text))
        let query = await IssueSearchTranslator.translate(text, members: members, now: now, translator: fake)
        XCTAssertNil(query)
    }

    func testAFailingModelIsNoQuery() async {
        let fake = Fake(.fail)
        let query = await IssueSearchTranslator.translate(text, members: members, now: now, translator: fake)
        XCTAssertNil(query)
    }

    func testAHangingModelEndsAtTheDeadline() async {
        let started = ContinuousClock.now
        let query = await IssueSearchTranslator.translate(
            text, members: members, now: now, translator: Fake(.hang), timeout: .milliseconds(200)
        )
        XCTAssertNil(query)
        XCTAssertLessThan(ContinuousClock.now - started, .seconds(5))
    }

    func testACancelEndsTheCallAtOnce() async {
        let members = members, now = now, text = text
        let started = ContinuousClock.now
        let call = Task {
            await IssueSearchTranslator.translate(text, members: members, now: now, translator: Fake(.hang), timeout: .seconds(60))
        }
        try? await Task.sleep(for: .milliseconds(100))
        call.cancel()
        let query = await call.value
        XCTAssertNil(query)
        XCTAssertLessThan(ContinuousClock.now - started, .seconds(5))
    }

    func testTheDeadlineIsTheDecidedOne() {
        XCTAssertEqual(IssueSearchTranslation.Limits.timeoutSeconds, 15)
    }
}
