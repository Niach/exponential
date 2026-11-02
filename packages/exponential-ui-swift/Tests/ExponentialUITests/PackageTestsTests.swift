import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-196: `fixtures/package-tests.json` (`runs`, `validate`) over the FFI:
/// `runs` through `runPackageTestsJson`, `validate` through
/// `validatePackageJson` (a failing test = a `/tests/<i>` install issue).
/// The TS reference, the Rust core, the FFI facade and Compose's
/// `PackageTestsTest` replay the same cases.
final class PackageTestsTests: XCTestCase {
    func same(_ got: JSONValue, _ want: JSONValue, _ name: String) {
        XCTAssertTrue(jsonEqual(a: got.json, b: want.json), "\(name): got \(got.json.prefix(300)), want \(want.json.prefix(300))")
    }

    /// A validate case's package: the base with its `with` keys replaced.
    static func validateCasePackage(_ base: JSONValue, _ with: JSONValue?) -> JSONValue {
        var pkg = base.object ?? [:]
        for (k, v) in with?.object ?? [:] { pkg[k] = v }
        return .object(pkg)
    }

    func testRuns() throws {
        let f = try Fixtures.json("package-tests.json")
        var pkg = f["package"]?.object ?? [:]
        pkg["tests"] = f["runs"]?["tests"]
        let got = try runPackageTestsJson(packageJson: JSONValue.object(pkg).json)
        same(JSONValue.parse(got), f["runs"]?["results"] ?? .null, "runs")
        XCTAssertEqual(try runPackageTestsJson(packageJson: (f["package"] ?? .null).json), "[]")
    }

    func testValidate() throws {
        let f = try Fixtures.json("package-tests.json")
        let base = f["package"] ?? .null
        let cases = f["validate"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let pkg = Self.validateCasePackage(base, c["with"])
            let got = try validatePackageJson(packageJson: pkg.json, catalogIds: nil)
            same(JSONValue.parse(got), c["issues"] ?? .array([]), "validate: \(c["name"]?.string ?? "")")
        }
    }
}
