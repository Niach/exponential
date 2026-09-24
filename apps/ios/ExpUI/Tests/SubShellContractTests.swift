import SwiftUI
import XCTest
@testable import ExpUI

// EXP-1029 contract — sub-shell navigation on iOS, implemented by EXP-1042
// (web, IDE and Android carry the same case names).
//
// ExpUI's tests are plain XCTest with no view inspection, so the cases drive
// `SubShellNavigation` — the page stack the host and the rows both render off
// — rather than poking at pixels. Everything a sub-shell does that is not
// drawing lives there.
final class SubShellContractTests: XCTestCase {

    private func page(_ id: String, _ title: String) -> SubShellPage {
        SubShellPage(id: id, title: title) { Text(title) }
    }

    func testTappingTheRowSlidesTheChildPageInPlaceOfTheWholeCard() {
        var navigation = SubShellNavigation()
        XCTAssertFalse(navigation.isOpen)
        XCTAssertNil(navigation.current)

        navigation.push(page("workflow", "Workflow settings"))

        // The card is gone: the host draws THIS page instead of it.
        XCTAssertTrue(navigation.isOpen)
        XCTAssertEqual(navigation.depth, 1)
        XCTAssertEqual(navigation.current?.id, "workflow")
        XCTAssertEqual(navigation.current?.title, "Workflow settings")
    }

    func testTheChildPageCarriesABackButtonOnTopThatReturnsToTheCard() {
        var navigation = SubShellNavigation()
        navigation.push(page("workflow", "Workflow settings"))

        navigation.back()

        XCTAssertFalse(navigation.isOpen)
        XCTAssertEqual(navigation.depth, 0)
        XCTAssertNil(navigation.current)
        // Back at the card, back is a no-op rather than an underflow.
        navigation.back()
        XCTAssertEqual(navigation.depth, 0)
    }

    func testASubShellInsideTheChildPageSlidesOneLevelDeeper() {
        var navigation = SubShellNavigation()
        navigation.push(page("workflow", "Workflow settings"))
        navigation.push(page("models", "Models"))

        // Only the TOP page draws: a deeper page hides its parent's rows and
        // its parent's back header.
        XCTAssertEqual(navigation.depth, 2)
        XCTAssertEqual(navigation.current?.id, "models")

        // And back returns ONE level, to the enclosing page.
        navigation.back()
        XCTAssertEqual(navigation.depth, 1)
        XCTAssertEqual(navigation.current?.id, "workflow")
    }

    func testADisabledRowNeverOpens() {
        var navigation = SubShellNavigation()
        navigation.push(page("workflow", "Workflow settings"), disabled: true)
        XCTAssertFalse(navigation.isOpen)

        // Nor does it disturb a page that is already open.
        navigation.push(page("workflow", "Workflow settings"))
        navigation.push(page("models", "Models"), disabled: true)
        XCTAssertEqual(navigation.depth, 1)
        XCTAssertEqual(navigation.current?.id, "workflow")
    }

    /// The page is pushed as a CLOSURE, never as a view snapshot frozen at
    /// tap time: the host re-invokes it on every render, so the bindings the
    /// caller handed it (the workflow pickers' drafts) stay live.
    func testAnOpenPageFollowsItsLiveBindings() {
        var model = "opus"
        var rendered: [String] = []
        var navigation = SubShellNavigation()
        navigation.push(SubShellPage(id: "workflow", title: "Workflow settings") {
            rendered.append(model)
            return Text(model)
        })

        _ = navigation.current?.content()
        model = "fable"
        _ = navigation.current?.content()

        XCTAssertEqual(rendered, ["opus", "fable"])
    }
}
