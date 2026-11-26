import Foundation
import XCTest
@testable import ExpCore

// #EXP-2143 follow-up: "Confirm it's you", locked ×4 against
// `domain-contract/fixtures/step-up.json` — the same flows replay on the web
// (`step-up.test.ts`), Android (StepUpTest) and the desktop (domain::step_up).
final class StepUpTests: XCTestCase {
    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/step-up.json")
        let object = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(object as? [String: Any])
    }

    private func methods(_ value: Any?) throws -> [StepUp.Method] {
        try XCTUnwrap(value as? [String]).map { try XCTUnwrap(StepUp.Method(rawValue: $0), $0) }
    }

    private func optionalString(_ row: [String: Any], _ key: String) -> String? {
        guard let value = row[key], !(value is NSNull) else { return nil }
        return value as? String
    }

    private func state(_ row: [String: Any]) throws -> StepUp.State {
        StepUp.State(
            sheet: try XCTUnwrap(StepUp.Sheet(rawValue: try XCTUnwrap(row["sheet"] as? String))),
            method: optionalString(row, "method").flatMap(StepUp.Method.init(rawValue:)),
            methods: try methods(row["methods"]),
            error: optionalString(row, "error"),
            action: try XCTUnwrap(StepUp.Action(rawValue: try XCTUnwrap(row["action"] as? String))),
            retried: try XCTUnwrap(row["retried"] as? Bool)
        )
    }

    private func event(_ row: [String: Any]) throws -> StepUp.Event {
        switch try XCTUnwrap(row["on"] as? String) {
        case "refused":
            return .refused(code: try XCTUnwrap(row["code"] as? String), methods: try methods(row["methods"]))
        case "choose":
            return .choose(try XCTUnwrap(StepUp.Method(rawValue: try XCTUnwrap(row["method"] as? String))))
        case "submit": return .submit
        case "proved": return .proved
        case "rejected": return .rejected(code: try XCTUnwrap(row["code"] as? String))
        case "cancel": return .cancel
        case "done": return .done
        case let other:
            throw NSError(domain: "StepUpTests", code: 1, userInfo: [NSLocalizedDescriptionKey: "unknown event \(other)"])
        }
    }

    func testInitialState() throws {
        let initial = try XCTUnwrap(try fixture()["initial"] as? [String: Any])
        XCTAssertEqual(StepUp.State(), try state(initial))
    }

    func testCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: Any])
        XCTAssertEqual(StepUp.title, copy["title"] as? String)
        XCTAssertEqual(StepUp.passkeyButton, copy["passkey"] as? String)
        XCTAssertEqual(StepUp.confirm, copy["confirm"] as? String)
        XCTAssertEqual(StepUp.resend, copy["resend"] as? String)
        XCTAssertEqual(StepUp.notFreshCopy, copy["notFresh"] as? String)
        let description = try XCTUnwrap(copy["description"] as? [String: String])
        let field = try XCTUnwrap(copy["field"] as? [String: String])
        let switches = try XCTUnwrap(copy["switch"] as? [String: String])
        for method in StepUp.Method.allCases {
            XCTAssertEqual(
                method.description(email: "dev@example.com"),
                description[method.rawValue]?.replacingOccurrences(of: "{email}", with: "dev@example.com"),
                method.rawValue
            )
            XCTAssertEqual(method.field, field[method.rawValue], method.rawValue)
            XCTAssertEqual(method.switchLabel, switches[method.rawValue], method.rawValue)
        }
    }

    func testErrorsMatchTheFixture() throws {
        let errors = try XCTUnwrap(try fixture()["errors"] as? [String: String])
        for (code, sentence) in errors where code != "default" {
            XCTAssertEqual(StepUp.errorCopy(code), sentence, code)
        }
        XCTAssertEqual(StepUp.errorCopy("SOMETHING_ELSE"), errors["default"])
        XCTAssertNil(StepUp.errorCopy(StepUp.cancelled), "a dismissed passkey prompt is no error")
    }

    func testEveryFlow() throws {
        let flows = try XCTUnwrap(try fixture()["flows"] as? [[String: Any]])
        XCTAssertFalse(flows.isEmpty)
        for flow in flows {
            let name = try XCTUnwrap(flow["name"] as? String)
            var current = StepUp.State()
            for row in try XCTUnwrap(flow["events"] as? [[String: Any]]) {
                current = StepUp.reduce(current, try event(row))
            }
            XCTAssertEqual(current, try state(try XCTUnwrap(flow["expect"] as? [String: Any])), name)
        }
    }

    /// The iOS flow is ours: the existing passkey proves it, the add resumes
    /// and finishes.
    func testTheIosFlowIsPasskeyProofThenResume() throws {
        let flows = try XCTUnwrap(try fixture()["flows"] as? [[String: Any]])
        let ios = try XCTUnwrap(flows.first { $0["platform"] as? String == "ios" })
        let events = try XCTUnwrap(ios["events"] as? [[String: Any]]).map(event)
        var current = StepUp.reduce(StepUp.State(), events[0])
        XCTAssertEqual(current.method, .passkey)
        XCTAssertEqual(current.sheet, .open)
        current = StepUp.reduce(current, events[1])
        current = StepUp.reduce(current, events[2])
        XCTAssertEqual(current.action, .resume)
        XCTAssertTrue(current.retried)
    }
}
