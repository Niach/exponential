import UIKit
import XCTest
@testable import ExpUI

/// The offline banner above the navigation stack (`TopBannerStack`) moves the
/// stack; every stacked page must lay out again to read its new safe area
/// (the app run view kept a band the banner's height under its header).
@MainActor
final class ControllerRelayoutTests: XCTestCase {
    func testTheTreeReachesEveryStackedPage() {
        let root = UIViewController()
        let first = UIViewController()
        let pushed = UIViewController()
        let nav = UINavigationController(rootViewController: first)
        nav.pushViewController(pushed, animated: false)
        let nested = UIViewController()
        pushed.addChild(nested)
        root.addChild(nav)
        XCTAssertEqual(root.controllerTree.map(ObjectIdentifier.init), [root, nav, first, pushed, nested].map(ObjectIdentifier.init))
    }

    func testRelayoutMarksEveryPageForLayout() {
        let first = UIViewController()
        let pushed = UIViewController()
        let nav = UINavigationController(rootViewController: first)
        nav.pushViewController(pushed, animated: false)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.rootViewController = nav
        window.isHidden = false
        for controller in nav.controllerTree { controller.view.layoutIfNeeded() }
        XCTAssertFalse(pushed.view.layer.needsLayout())
        XCTAssertFalse(first.view.layer.needsLayout())
        nav.relayoutControllerTree()
        XCTAssertTrue(nav.view.layer.needsLayout())
        XCTAssertTrue(pushed.view.layer.needsLayout())
        XCTAssertTrue(first.view.layer.needsLayout())
        window.isHidden = true
    }
}
