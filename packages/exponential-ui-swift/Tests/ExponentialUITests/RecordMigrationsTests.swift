import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// eApps R8 (VAPP-233): `fixtures/record-migrations.json` through the SDK
/// (the core over the FFI). The TS reference and the Rust core replay the
/// same cases.
final class RecordMigrationsTests: XCTestCase {
    func testFixture() throws {
        let f = try Fixtures.json("record-migrations.json")
        let migrate = f["migrateCases"]?.array ?? []
        XCTAssertFalse(migrate.isEmpty)
        for c in migrate {
            let got = RecordMigrations.migrate(package: c["package"] ?? .null, version: c["version"]?.string ?? "", kind: c["kind"]?.string ?? "", data: c["data"] ?? .null)
            let want = c["expected"] ?? .null
            XCTAssertTrue(jsonEqual(a: got.json, b: want.json), "\(c["name"]?.string ?? ""): got \(got.json.prefix(300))")
        }
        for c in f["refusalCases"]?.array ?? [] {
            XCTAssertEqual(RecordMigrations.refusal(before: c["before"] ?? .null, after: c["after"] ?? .null), c["expected"]?.string, c["name"]?.string ?? "")
        }
    }
}
