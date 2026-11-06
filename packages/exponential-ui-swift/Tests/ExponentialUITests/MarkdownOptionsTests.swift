import XCTest
import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// #VAPP-258 step 5 (`catalog/host.json` markdown.options): the host's render
/// options reach the measurer exactly as the painter (measure = paint).
@MainActor
final class MarkdownOptionsTests: XCTestCase {
    final class OptionsHost: HostPlugin {
        let markdownOptions: MarkdownOptions
        init(_ options: MarkdownOptions) { markdownOptions = options }
    }

    func model(_ options: MarkdownOptions) throws -> SurfaceModel {
        var o = SurfaceOptions()
        // The theme as the repo has it (the xcframework's built-in copy may
        // predate the chat rule).
        let json = try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("themes/neutral.theme.json"), encoding: .utf8)
        o.theme = try ThemeHandle.load(json: json)
        return try SurfaceModel(id: "md", options: o, host: OptionsHost(options))
    }

    func measured(_ m: SurfaceModel, _ text: String) throws -> CGFloat {
        try m.setComponents(json: JSONValue.array([.object(["id": .string("root"), "component": .string("Markdown"), "text": .string(text)])]).json)
        m.setViewport(width: 320, height: 900)
        let root = try XCTUnwrap(m.nodes.first { $0.id == "root" })
        return m.frame(root.index).height
    }

    /// The painter's layout height (its props, styles and blocks).
    func painted(_ m: SurfaceModel, _ text: String) throws -> CGFloat {
        let root = try XCTUnwrap(m.nodes.first { $0.id == "root" })
        let props = MarkdownPainter.props(root.props, chat: m.host.markdownOptions.chat)
        let styles = MarkdownPainter.styles(theme: m.theme, mode: m.mode, body: m.textStyle(root.index), props: props)
        return Markdown.layout(MarkdownPainter.painted(text, model: m), styles, width: m.frame(root.index).width, text: MarkdownShaper(mono: m.theme?.monoFamily, chip: styles.chip)).height
    }

    func testChatDensityMeasuresAsItPaints() throws {
        let text = "# Title\n\nOne paragraph.\n\nTwo.\n\nThree."
        let doc = try model(MarkdownOptions())
        let chat = try model(MarkdownOptions(chat: true))
        let docH = try measured(doc, text)
        let chatH = try measured(chat, text)
        XCTAssertEqual(chatH, try painted(chat, text), accuracy: 0.5)
        XCTAssertEqual(docH, try painted(doc, text), accuracy: 0.5)
        XCTAssertNotEqual(chatH, docH, "the chat rhythm changes the height")
        let root = try XCTUnwrap(chat.nodes.first { $0.id == "root" })
        let styles = MarkdownPainter.styles(theme: chat.theme, mode: chat.mode, body: chat.textStyle(root.index), props: MarkdownPainter.props(root.props, chat: true))
        XCTAssertEqual(styles.body.lineHeight, 22)
    }

    func testHardBreaksMeasureEveryLine() throws {
        let soft = try measured(try model(MarkdownOptions()), "a\nb\nc")
        let hardModel = try model(MarkdownOptions(hardBreaks: true))
        let hard = try measured(hardModel, "a\nb\nc")
        XCTAssertEqual(hard, 3 * soft, accuracy: 0.5)
        XCTAssertEqual(hard, try painted(hardModel, "a\nb\nc"), accuracy: 0.5)
    }

    func testStreamingHoldsTheTailBack() throws {
        let m = try model(MarkdownOptions(streaming: true))
        XCTAssertEqual(Markdown.plain(MarkdownPainter.blocks("Say **bo", model: m)[0].inlines), "Say bo")
        XCTAssertEqual(try measured(m, "Say **bo"), try painted(m, "Say **bo"), accuracy: 0.5)
        XCTAssertEqual(MarkdownPainter.blocks("Say **bo", model: try model(MarkdownOptions()))[0].inlines.map(\.text).joined(), "Say **bo")
    }
}
