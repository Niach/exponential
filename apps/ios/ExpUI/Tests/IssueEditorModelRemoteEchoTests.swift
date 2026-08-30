import Foundation
import XCTest
import ExpUI

// EXP-689: the debounced autosave fires while the user keeps typing. The
// Electric echo of that save then carries text the editor has already moved
// past — it is OURS and must never raise the "Updated by someone else" reload
// banner (which shifted the whole layout for a moment on every autosave).
@MainActor
final class IssueEditorModelRemoteEchoTests: XCTestCase {
    private func typingModel(_ markdown: String) -> (IssueEditorModel, UUID) {
        let model = IssueEditorModel()
        model.load(markdown: markdown, baseURL: nil)
        let id = model.blocks.compactMap { block -> UUID? in
            if case .text(let id, _) = block { return id }
            return nil
        }.first!
        model.setFocused(id)
        return (model, id)
    }

    func testEchoOfAnInFlightSaveNeverRaisesTheBanner() {
        let (model, id) = typingModel("Hello")
        model.updateText(id: id, content: NSAttributedString(string: "Hello wor"))
        // The autosave fires with the text so far…
        model.markSaving("Hello wor")
        // …the user keeps typing…
        model.updateText(id: id, content: NSAttributedString(string: "Hello world"))
        // …and the save's echo lands before `markSaved` ran.
        model.applyRemote(markdown: "Hello wor", baseURL: nil)
        XCTAssertNil(model.pendingRemoteMarkdown)
        XCTAssertEqual(model.currentMarkdown(), "Hello world")
        XCTAssertTrue(model.isDirty)
    }

    func testEchoOfTheLastSavedTextNeverRaisesTheBanner() {
        let (model, id) = typingModel("Hello")
        model.updateText(id: id, content: NSAttributedString(string: "Hello wor"))
        model.markSaving("Hello wor")
        model.markSaved("Hello wor")
        model.updateText(id: id, content: NSAttributedString(string: "Hello world"))
        model.applyRemote(markdown: "Hello wor", baseURL: nil)
        XCTAssertNil(model.pendingRemoteMarkdown)
        XCTAssertEqual(model.currentMarkdown(), "Hello world")
    }

    func testAForeignEditStillStashesBehindTheBanner() {
        let (model, id) = typingModel("Hello")
        model.updateText(id: id, content: NSAttributedString(string: "Hello wor"))
        model.markSaving("Hello wor")
        model.updateText(id: id, content: NSAttributedString(string: "Hello world"))
        model.applyRemote(markdown: "Hello from a teammate", baseURL: nil)
        XCTAssertEqual(model.pendingRemoteMarkdown, "Hello from a teammate")
        XCTAssertEqual(model.currentMarkdown(), "Hello world")
    }
}
