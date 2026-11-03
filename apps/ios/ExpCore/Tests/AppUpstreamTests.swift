import Foundation
import XCTest
@testable import ExpCore

// #VAPP-230: upstream read bindings, locked ×4 against
// packages/emcp/fixtures/upstream.json `binding` (TS `upstream.test.ts`):
// the scheme, the re-read period, the banner text and the disconnected
// error pin; `bannerCases` replay through `AppUpstream.banners`.
final class AppUpstreamTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/emcp/fixtures/upstream.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func binding() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["binding"] as? [String: Any])
    }

    func testConstantsMatchTheFixture() throws {
        let b = try binding()
        XCTAssertEqual(b["scheme"] as? String, AppUpstream.scheme)
        XCTAssertEqual(b["cacheSeconds"] as? Int, AppUpstream.cacheSeconds)
        XCTAssertEqual(b["banner"] as? String, AppUpstream.banner)
        let errors = try XCTUnwrap(try fixture()["errors"] as? [String: String])
        XCTAssertEqual(errors["disconnected"], AppUpstream.disconnected)
    }

    func testBannerCasesReplay() throws {
        let cases = try XCTUnwrap(try binding()["bannerCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? ""
            let answers: [AppUpstream.Answer] = try XCTUnwrap(c["answers"] as? [[String: Any]], name).map { (a: [String: Any]) -> AppUpstream.Answer in
                if a["ok"] as? Bool == true {
                    return .ok(CardJSON(any: a["value"]) ?? .null)
                }
                return .refused(error: a["error"] as? String ?? "", server: a["server"] as? String ?? "")
            }
            let expected = try XCTUnwrap(c["expected"] as? [String], name)
            XCTAssertEqual(AppUpstream.banners(answers), expected, name)
        }
    }

    /// #VAPP-239: `device.bannerCases` through `AppUpstream.hostBanners`.
    func testDeviceBannerCasesReplay() throws {
        let device = try XCTUnwrap(try fixture()["device"] as? [String: Any])
        let copy = try XCTUnwrap(device["copy"] as? [String: String])
        XCTAssertEqual(copy["offline"], AppUpstream.offlineBanner)
        let cases = try XCTUnwrap(device["bannerCases"] as? [[String: Any]])
        for c in cases {
            let name = c["name"] as? String ?? ""
            let answers: [AppUpstream.Answer] = try XCTUnwrap(c["answers"] as? [[String: Any]], name).map { (a: [String: Any]) -> AppUpstream.Answer in
                if a["ok"] as? Bool == true { return .ok(CardJSON(any: a["value"]) ?? .null) }
                if let offline = a["offline"] as? String { return .offline(server: a["server"] as? String ?? "", device: offline) }
                return .refused(error: a["error"] as? String ?? "", server: a["server"] as? String ?? "")
            }
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]], name).map {
                AppUpstream.HostBanner(server: $0["server"] as? String ?? "", text: $0["text"] as? String ?? "", connect: $0["connect"] as? Bool ?? false)
            }
            XCTAssertEqual(AppUpstream.hostBanners(answers), expected, name)
        }
    }
}
