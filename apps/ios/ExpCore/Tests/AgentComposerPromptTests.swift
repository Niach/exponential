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

    func testTheLengthCapIsInclusive() {
        XCTAssertTrue(AgentComposerPrompt.withinLimit(""))
        XCTAssertTrue(
            AgentComposerPrompt.withinLimit(String(repeating: "a", count: AgentComposerPrompt.maxLength))
        )
        XCTAssertFalse(
            AgentComposerPrompt.withinLimit(String(repeating: "a", count: AgentComposerPrompt.maxLength + 1))
        )
    }

    // The ×4 submit label contract.
    func testSubmitTitlesFollowTheSubject() {
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .none), "Start chat")
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .issues(count: 1)), "Start coding")
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .issues(count: 2)), "Start batch · 2")
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .issues(count: 30)), "Start batch · 30")
        XCTAssertEqual(AgentComposerPrompt.submitTitle(for: .action), "Run action")
    }

    // EXP-825: the field placeholder — web `composerPlaceholder` byte for
    // byte. No subject asks for the message; a picked action with a
    // non-blank hint shows the hint; everything else the generic prompt.
    func testPlaceholderFollowsTheSubjectAndTheActionHint() {
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .none, actionHint: nil), "Ask the agent…"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .issues(count: 1), actionHint: nil),
            "Additional instructions (optional)…"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .issues(count: 3), actionHint: nil),
            "Additional instructions (optional)…"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .action, actionHint: nil),
            "Additional instructions (optional)…"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(
                for: .action, actionHint: "Scope: which platforms, which version"
            ),
            "Scope: which platforms, which version"
        )
    }

    // A blank hint is no hint; a padded one is shown trimmed. And a hint
    // never leaks onto the other subjects (the model only passes the PICKED
    // action's hint, but the rule guards it too).
    func testBlankHintsFallBackAndPaddedHintsTrim() {
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .action, actionHint: "   \n"),
            "Additional instructions (optional)…"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .action, actionHint: "  Which version?  "),
            "Which version?"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .none, actionHint: "Which version?"),
            "Ask the agent…"
        )
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .issues(count: 1), actionHint: "Which version?"),
            "Additional instructions (optional)…"
        )
    }

    func testTheCreateBuiltinHintReachesThePlaceholder() {
        let create = ActionDto.builtinCreateAction(teamId: "t-1")
        XCTAssertEqual(
            AgentComposerPrompt.placeholder(for: .action, actionHint: create.promptPlaceholder),
            "Describe the action — what it should do, and its name if you have one…"
        )
    }
}
