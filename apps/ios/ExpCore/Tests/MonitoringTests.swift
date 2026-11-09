import Foundation
import XCTest
@testable import ExpCore

// #LOGS-2: what the app may send to the monitoring tunnel, and how it reads
// the server's answer.
final class MonitoringTests: XCTestCase {
    typealias Scrubber = Monitoring.Scrubber

    // MARK: - Scrubber

    func testSensitiveKeys() {
        for key in [
            "Authorization", "authorization", "Cookie", "Set-Cookie", "x-api-key", "apiKey",
            "token", "access_token", "client_secret", "password", "ticket", "steerTicket",
            "session", "sessionToken", "dsn", "SENTRY_DSN",
        ] {
            XCTAssertTrue(Scrubber.isSensitive(key), key)
        }
        for key in ["url", "method", "status_code", "http.query", "reason", "teamSlug", "boardId"] {
            XCTAssertFalse(Scrubber.isSensitive(key), key)
        }
    }

    func testHeadersAreFiltered() {
        let headers = Scrubber.scrub(headers: [
            "Authorization": "Bearer abc",
            "Cookie": "better-auth.session_token=abc",
            "Content-Type": "application/json",
            "x-client-version": "ios/1.0",
        ])
        XCTAssertEqual(headers["Authorization"], Scrubber.filtered)
        XCTAssertEqual(headers["Cookie"], Scrubber.filtered)
        XCTAssertEqual(headers["Content-Type"], "application/json")
        XCTAssertEqual(headers["x-client-version"], "ios/1.0")
    }

    func testNestedDataIsFiltered() {
        let data = Scrubber.scrub([
            "url": "https://app.exponential.at/api/attachments/1?token=abc&w=200",
            "status_code": 401,
            "nested": ["password": "hunter2", "ok": "yes"],
            "list": [["secret": "x"], ["name": "y"]],
            "http.query": "offset=-1&ticket=abc",
        ])
        let url = try? XCTUnwrap(data["url"] as? String)
        XCTAssertEqual(URLComponents(string: url ?? "")?.queryItems?.first { $0.name == "token" }?.value, Scrubber.filtered)
        XCTAssertTrue(url?.hasPrefix("https://app.exponential.at/api/attachments/1?") == true)
        XCTAssertFalse(url?.contains("abc") == true)
        XCTAssertEqual(data["status_code"] as? Int, 401)
        let nested = data["nested"] as? [String: Any]
        XCTAssertEqual(nested?["password"] as? String, Scrubber.filtered)
        XCTAssertEqual(nested?["ok"] as? String, "yes")
        let list = data["list"] as? [[String: Any]]
        XCTAssertEqual(list?[0]["secret"] as? String, Scrubber.filtered)
        XCTAssertEqual(list?[1]["name"] as? String, "y")
        XCTAssertEqual(data["http.query"] as? String, "offset=-1&ticket=[Filtered]")
    }

    func testUrlCredentialsAreDropped() {
        XCTAssertEqual(
            Scrubber.scrubUrl("https://user:pw@example.com/path?a=1"),
            "https://example.com/path?a=1"
        )
        XCTAssertEqual(Scrubber.scrubUrl("https://example.com/v1/shape?table=issues"), "https://example.com/v1/shape?table=issues")
        XCTAssertEqual(Scrubber.scrub("plain message") as? String, "plain message")
    }

    // MARK: - Requests

    func testOwnRequestsAreTheSignedInInstances() {
        let origins = ["https://app.exponential.at", "http://192.168.1.5:3000/"]
        XCTAssertTrue(Monitoring.isOwnRequest("https://app.exponential.at/api/trpc/issues.list", origins: origins))
        XCTAssertTrue(Monitoring.isOwnRequest("https://APP.exponential.at/api/shapes/issues?offset=-1", origins: origins))
        XCTAssertTrue(Monitoring.isOwnRequest("http://192.168.1.5:3000/api/shapes/boards", origins: origins))
        XCTAssertFalse(Monitoring.isOwnRequest("http://192.168.1.5:4000/api", origins: origins))
        XCTAssertFalse(Monitoring.isOwnRequest("https://firebaseinstallations.googleapis.com/v1", origins: origins))
        XCTAssertFalse(Monitoring.isOwnRequest("https://app.exponential.at/api/monitoring", origins: origins))
        XCTAssertTrue(Monitoring.isTunnelRequest("https://app.exponential.at/api/monitoring"))
        XCTAssertFalse(Monitoring.isTunnelRequest("https://app.exponential.at/api/trpc/monitoring"))
    }

    func testTargetPatternsMatchOnlyTheOrigin() throws {
        let patterns = Monitoring.targetPatterns(origins: ["https://app.exponential.at/", "https://app.exponential.at"])
        XCTAssertEqual(patterns.count, 1)
        let regex = try NSRegularExpression(pattern: patterns[0])
        func matches(_ s: String) -> Bool { regex.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) != nil }
        XCTAssertTrue(matches("https://app.exponential.at/api/shapes/issues"))
        XCTAssertFalse(matches("https://app.exponential.at.evil.com/api"))
    }

    func testUrls() {
        XCTAssertEqual(
            Monitoring.configUrl(origin: "https://app.exponential.at/")?.absoluteString,
            "https://app.exponential.at/api/monitoring/config?platform=ios"
        )
        XCTAssertEqual(Monitoring.tunnelUrl(origin: "https://next.exponential.at")?.absoluteString, "https://next.exponential.at/api/monitoring")
    }

    // MARK: - Config

    func testConfigDecoding() {
        let on = Monitoring.decodeConfig(Data(#"{"enabled":true,"dsn":"https://k@h/1","environment":"prod","tracesSampleRate":0.25}"#.utf8))
        XCTAssertTrue(on.isActive)
        XCTAssertEqual(on.resolvedTracesSampleRate, 0.25)
        XCTAssertEqual(on.resolvedAppHangTimeout, 2)
        let off = Monitoring.decodeConfig(Data(#"{"enabled":false}"#.utf8))
        XCTAssertFalse(off.isActive)
        XCTAssertFalse(Monitoring.decodeConfig(Data("<html>".utf8)).isActive)
        XCTAssertFalse(Monitoring.Config(enabled: true).isActive)
        XCTAssertEqual(Monitoring.Config(enabled: true, dsn: "x", tracesSampleRate: 7).resolvedTracesSampleRate, 0.1)
    }

    func testReleaseAndEnvironment() {
        XCTAssertEqual(
            Monitoring.release(info: ["CFBundleShortVersionString": "0.14.119", "CFBundleVersion": "316"]),
            "exponential-ios@0.14.119+316"
        )
        let config = Monitoring.Config(enabled: true, dsn: "x", environment: "prod")
        XCTAssertEqual(Monitoring.environment(config: config, origin: "https://app.exponential.at", isDebug: true), "dev")
        XCTAssertEqual(Monitoring.environment(config: config, origin: "https://app.exponential.at", isDebug: false), "prod")
        let unnamed = Monitoring.Config(enabled: true, dsn: "x")
        XCTAssertEqual(Monitoring.environment(config: unnamed, origin: "https://next.exponential.at/", isDebug: false), "staging")
        XCTAssertEqual(Monitoring.environment(config: unnamed, origin: "https://app.exponential.at", isDebug: false), "prod")
    }

    func testLocalStateDefaultsOnAndCaches() throws {
        let suite = "monitoring-tests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        XCTAssertTrue(Monitoring.sendDiagnostics(defaults))
        defaults.set(false, forKey: Monitoring.sendDiagnosticsKey)
        XCTAssertFalse(Monitoring.sendDiagnostics(defaults))
        XCTAssertNil(Monitoring.cached(defaults))
        let cached = Monitoring.Cached(origin: "https://app.exponential.at", config: .init(enabled: true, dsn: "x"))
        Monitoring.store(cached, defaults)
        XCTAssertEqual(Monitoring.cached(defaults), cached)
    }
}
