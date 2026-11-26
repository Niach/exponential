import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

/// genUI round 4: `fixtures/box-backgrounds.json` on SwiftUI: the radial
/// circle (`GradientPaint.radialRadius`, the `RadialGradient` end radius),
/// the picture placement (`PicturePaint.rect`) and the FFI JSON → paint
/// parse (`visual`). The TS reference, the Rust core and Kotlin's
/// `BoxBackgroundsTest` replay the same file.
final class BoxBackgroundsTests: XCTestCase {
    private func f(_ v: JSONValue?) -> CGFloat { CGFloat(v?.number ?? .nan) }

    func testRadial() throws {
        let cases = try Fixtures.json("box-backgrounds.json")["radial"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let size = c["size"]?.array ?? [], centre = c["centre"]?.array ?? []
            let (w, h, cx, cy) = (f(size[0]), f(size[1]), f(centre[0]), f(centre[1]))
            let r = GradientPaint.radialRadius(width: w, height: h, cx: cx, cy: cy)
            XCTAssertEqual(r, f(c["radius"]), accuracy: 1e-3, c["name"]?.string ?? "")
            for s in c["samples"]?.array ?? [] {
                let p = s.array ?? []
                let t = r > 0 ? hypot(f(p[0]) - cx * w, f(p[1]) - cy * h) / r : 0
                XCTAssertEqual(t, f(p[2]), accuracy: 1e-4, c["name"]?.string ?? "")
            }
        }
    }

    func testPicture() throws {
        for c in try Fixtures.json("box-backgrounds.json")["picture"]?.array ?? [] {
            let size = c["size"]?.array ?? [], image = c["image"]?.array ?? [], focal = c["focal"]?.array ?? [], want = c["rect"]?.array ?? []
            let r = PicturePaint.rect(contain: c["fit"]?.string == "contain", box: CGSize(width: f(size[0]), height: f(size[1])), image: CGSize(width: f(image[0]), height: f(image[1])), focalX: f(focal[0]), focalY: f(focal[1]))
            let name = c["name"]?.string ?? ""
            XCTAssertEqual(r.minX, f(want[0]), accuracy: 1e-4, name)
            XCTAssertEqual(r.minY, f(want[1]), accuracy: 1e-4, name)
            XCTAssertEqual(r.width, f(want[2]), accuracy: 1e-4, name)
            XCTAssertEqual(r.height, f(want[3]), accuracy: 1e-4, name)
        }
    }

    func testVisual() throws {
        for c in try Fixtures.json("box-backgrounds.json")["visual"]?.array ?? [] {
            let name = c["name"]?.string ?? ""
            if let image = c["backgroundImage"] {
                let p = try XCTUnwrap(PicturePaint.parse(image.json), name)
                XCTAssertEqual(p.src, image["src"]?.string, name)
                XCTAssertEqual(p.contain, image["fit"]?.string == "contain", name)
                XCTAssertEqual(p.focalX, f(image["focalX"]), accuracy: 1e-6, name)
                XCTAssertEqual(p.focalY, f(image["focalY"]), accuracy: 1e-6, name)
                XCTAssertEqual(p.scrim == nil, image["scrim"] == nil, name)
            }
            if let gradient = c["backgroundGradient"] {
                let g = try XCTUnwrap(GradientPaint.parse(gradient.json), name)
                XCTAssertEqual(g.stops.count, gradient["stops"]?.array?.count, name)
                if gradient["kind"]?.string == "radial" {
                    let centre = gradient["centre"]?.array ?? []
                    XCTAssertEqual(g.centre?.x ?? -1, f(centre[0]), accuracy: 1e-6, name)
                    XCTAssertEqual(g.centre?.y ?? -1, f(centre[1]), accuracy: 1e-6, name)
                } else {
                    XCTAssertNil(g.centre, name)
                    XCTAssertEqual(g.angle, f(gradient["angle"]), accuracy: 1e-6, name)
                }
            }
        }
    }
}
