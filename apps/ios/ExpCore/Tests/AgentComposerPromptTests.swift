import Foundation
import XCTest
@testable import ExpCore

// EXP-825: the composer's free text on the wire — the steer embed format
// (byte-identical ×4), absent when there is nothing to say, capped by the
// contract — and the ×4 submit labels.
final class AgentComposerPromptTests: XCTestCase {
    func testTheCapsAreTheContractsStartPrompt() {
        XCTAssertEqual(AgentComposerPrompt.maxLength, DomainContract.startPromptMaxLength)
        XCTAssertEqual(AgentComposerPrompt.maxLength, 16384)
        XCTAssertEqual(AgentComposerPrompt.maxImages, DomainContract.startPromptMaxImages)
        XCTAssertEqual(AgentComposerPrompt.maxImages, 4)
        // The steer composer carries the same four.
        XCTAssertEqual(AgentComposerPrompt.maxImages, SteerImageMessage.maxImages)
    }

    // Blank text and no images = NO prompt (the key is omitted), never "".
    func testBlankTextWithoutImagesIsAbsent() {
        XCTAssertNil(AgentComposerPrompt.build(text: "", attachmentIds: []))
        XCTAssertNil(AgentComposerPrompt.build(text: "   \n\t ", attachmentIds: []))
    }

    func testTextIsTrimmedAndSentAsIs() {
        XCTAssertEqual(
            AgentComposerPrompt.build(text: "  Refactor the parser  \n", attachmentIds: []),
            "Refactor the parser"
        )
    }

    // Images ride the exact `SteerImageMessage.build` shape: prose, blank
    // line, one embed line per id — so the desktop's localizer and the
    // server's embed validation read it like a steer message.
    func testImagesEmbedLikeASteerMessage() {
        let prompt = AgentComposerPrompt.build(
            text: "Crop [Image #1] like this", attachmentIds: ["att-1", "att-2"]
        )
        XCTAssertEqual(
            prompt,
            "Crop [Image #1] like this\n\n![image](/api/attachments/att-1)\n![image](/api/attachments/att-2)"
        )
        XCTAssertEqual(
            prompt,
            SteerImageMessage.build(text: "Crop [Image #1] like this", attachmentIds: ["att-1", "att-2"])
        )
        // Images alone are a prompt too (a chat may be just a screenshot).
        XCTAssertEqual(
            AgentComposerPrompt.build(text: "", attachmentIds: ["att-1"]),
            "![image](/api/attachments/att-1)"
        )
        // And the steer parser gets the ids back in order.
        let parsed = SteerImageMessage.parse(prompt ?? "")
        XCTAssertEqual(parsed.attachmentIds, ["att-1", "att-2"])
        XCTAssertEqual(parsed.text, "Crop [Image #1] like this")
    }

    // Wave D: a start prompt carries files after the images; files alone are
    // a prompt too.
    func testFilesFollowTheImagesAndAreAPromptAlone() {
        XCTAssertEqual(
            AgentComposerPrompt.build(
                text: "Read this",
                attachmentIds: ["att-1"],
                files: [.init(id: "att-2", name: "spec.pdf")]
            ),
            "Read this\n\n![image](/api/attachments/att-1)\n[spec.pdf](/api/attachments/att-2)"
        )
        XCTAssertEqual(
            AgentComposerPrompt.build(text: " ", attachmentIds: [], files: [.init(id: "att-2", name: "spec.pdf")]),
            "[spec.pdf](/api/attachments/att-2)"
        )
    }

    func testTheLengthCapIsInclusive() {
        XCTAssertTrue(AgentComposerPrompt.withinLimit(""))
        XCTAssertTrue(
            AgentComposerPrompt.withinLimit(String(repeating: "a", count: AgentComposerPrompt.maxLength))
        )
        XCTAssertFalse(
            AgentComposerPrompt.withinLimit(String(repeating: "a", count: AgentComposerPrompt.maxLength + 1))
        )
    }

    // The submit label per subject (web `submitLabelFor`).
    func testSubmitTitlesFollowTheSubject() {
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .none), "Start chat")
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .issues(count: 1)), "Start coding")
        // Several issues are still ONE thread.
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .issues(count: 3)), "Start coding")
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .prompt), "Start session")
        // NEW-3: a message into an agent's DM.
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .agent), "Send")
    }

    // EXP-1038: the headline verb over the composer — the contract's copy;
    // a prompt's or an agent's chip IS the headline (no verb).
    func testHeadlinesFollowTheSubject() {
        XCTAssertEqual(AgentComposerPrompt.headline(for: .none), DomainContract.composerUiChatHeadline)
        XCTAssertEqual(AgentComposerPrompt.headline(for: .issues(count: 1)), "Implement")
        XCTAssertEqual(
            AgentComposerPrompt.headline(for: .issues(count: 7)), DomainContract.composerUiImplementHeadline
        )
        XCTAssertEqual(AgentComposerPrompt.headline(for: .prompt), "")
        XCTAssertEqual(AgentComposerPrompt.headline(for: .agent), "")
    }

    // EXP-825: the field placeholder — web `composerPlaceholder`.
    func testPlaceholderFollowsTheSubject() {
        XCTAssertEqual(AgentComposerPrompt.placeholder(for: .none), "Ask the agent…")
        for subject: AgentComposerPrompt.Subject in [.issues(count: 1), .issues(count: 3), .prompt, .agent] {
            XCTAssertEqual(AgentComposerPrompt.placeholder(for: subject), "Additional instructions (optional)…")
        }
    }
}
