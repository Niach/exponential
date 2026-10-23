import Foundation
import XCTest
@testable import ExpCore

// NEW-2: THE schedule picker, locked ×4 (TS `schedule-picker.test.ts`, desktop
// `domain::schedule_picker`, Android `SchedulePickerTest`) against
// fixtures/schedule-picker.json: `cronCases` both ways, `parseCases`,
// `weekdaysLabelCases`, `summaryCases`, the copy, the test ids, the defaults.
final class SchedulePickerTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/schedule-picker.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func schedule(_ object: Any?) throws -> Schedule? {
        guard let object = object as? [String: Any] else { return nil }
        return Schedule(
            repeat: try XCTUnwrap(object["repeat"] as? String),
            every: try XCTUnwrap(object["every"] as? Int),
            minute: try XCTUnwrap(object["minute"] as? Int),
            hour: try XCTUnwrap(object["hour"] as? Int),
            weekdays: try XCTUnwrap(object["weekdays"] as? [Int]),
            dayOfMonth: try XCTUnwrap(object["dayOfMonth"] as? Int)
        )
    }

    func testCronCasesRoundTrip() throws {
        let cases = try XCTUnwrap(try fixture()["cronCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let expected = try XCTUnwrap(try schedule(testCase["schedule"]), name)
            let cron = try XCTUnwrap(testCase["cron"] as? String)
            XCTAssertEqual(SchedulePicker.cronFromSchedule(expected), cron, name)
            XCTAssertEqual(SchedulePicker.scheduleFromCron(cron), expected, name)
        }
    }

    func testParseCases() throws {
        let cases = try XCTUnwrap(try fixture()["parseCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let cron = try XCTUnwrap(testCase["cron"] as? String)
            XCTAssertEqual(SchedulePicker.scheduleFromCron(cron), try schedule(testCase["schedule"]), cron)
        }
    }

    func testWeekdaysLabelCases() throws {
        let cases = try XCTUnwrap(try fixture()["weekdaysLabelCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let weekdays = try XCTUnwrap(testCase["weekdays"] as? [Int])
            XCTAssertEqual(SchedulePicker.weekdaysLabel(weekdays), testCase["expected"] as? String, "\(weekdays)")
        }
    }

    func testSummaryCases() throws {
        let cases = try XCTUnwrap(try fixture()["summaryCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let cron = try XCTUnwrap(testCase["cron"] as? String)
            XCTAssertEqual(
                SchedulePicker.scheduleSummary(cron: cron, tz: testCase["tz"] as? String),
                testCase["expected"] as? String,
                cron
            )
        }
    }

    func testCopyTestIdsAndDefaultsMatchTheFixture() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        typealias C = SchedulePicker.Copy
        XCTAssertEqual(copy["rows"] as? [String: String], [
            "repeat": C.rowRepeat, "days": C.rowDays, "day": C.rowDay, "minute": C.rowMinute,
            "time": C.rowTime, "timeZone": C.rowTimeZone, "cron": C.rowCron, "every": C.rowEvery,
        ])
        XCTAssertEqual(copy["repeat"] as? [String: String], C.repeatLabels)
        XCTAssertEqual(copy["repeat"] as? [String: String], [
            "minutes": C.repeatMinutes, "hourly": C.repeatHourly, "daily": C.repeatDaily, "weekly": C.repeatWeekly,
            "monthly": C.repeatMonthly, "custom": C.repeatCustom,
        ])
        XCTAssertEqual(copy["weekdays"] as? [String], C.weekdays)
        XCTAssertEqual(copy["weekdaysShort"] as? [String], C.weekdaysShort)
        XCTAssertEqual(copy["weekdaysAll"] as? String, C.weekdaysAll)
        XCTAssertEqual(copy["weekdaysWork"] as? String, C.weekdaysWork)
        XCTAssertEqual(copy["weekdaysWeekend"] as? String, C.weekdaysWeekend)
        XCTAssertEqual(copy["dayOfMonth"] as? String, C.dayOfMonth)
        XCTAssertEqual(copy["minuteOfHour"] as? String, C.minuteOfHour)
        XCTAssertEqual(copy["timeZonePlaceholder"] as? String, C.timeZonePlaceholder)
        XCTAssertEqual(copy["cronPlaceholder"] as? String, C.cronPlaceholder)
        XCTAssertEqual(copy["summary"] as? [String: String], [
            "minutes": C.summaryMinutes, "hourly": C.summaryHourly, "daily": C.summaryDaily, "weekly": C.summaryWeekly,
            "weeklyOne": C.summaryWeeklyOne, "monthly": C.summaryMonthly,
            "custom": C.summaryCustom, "zoned": C.summaryZoned,
        ])
        XCTAssertEqual(copy["errors"] as? [String: String], ["noDays": C.errorNoDays, "cron": C.errorCron])
        // Every copy key is mirrored (a new key fails here).
        XCTAssertEqual(Set(copy.keys), [
            "rows", "repeat", "weekdays", "weekdaysShort", "weekdaysAll", "weekdaysWork",
            "weekdaysWeekend", "dayOfMonth", "minuteOfHour", "timeZonePlaceholder",
            "cronPlaceholder", "summary", "errors", "every",
        ])
        XCTAssertEqual(copy["every"] as? String, C.every)
        XCTAssertEqual(SchedulePicker.everyLabel(15), "15 minutes")

        typealias T = SchedulePicker.TestIds
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "repeat": T.repeat, "days": T.days, "day": T.day, "minute": T.minute,
            "time": T.time, "timeZone": T.timeZone, "cron": T.cron, "every": T.every,
        ])
        XCTAssertEqual(fixture["everyValues"] as? [Int], SchedulePicker.everyValues)
        XCTAssertEqual(fixture["repeatValues"] as? [String], SchedulePicker.repeatValues)
        XCTAssertEqual(fixture["dayOfMonthMax"] as? Int, SchedulePicker.dayOfMonthMax)
        XCTAssertEqual(try schedule(fixture["defaults"]), SchedulePicker.defaults())
        XCTAssertEqual(SchedulePicker.dayOfMonthLabel(3), "Day 3")
        XCTAssertEqual(SchedulePicker.minuteOfHourLabel(5), ":05")
        XCTAssertEqual(SchedulePicker.timeLabel(hour: 9, minute: 5), "09:05")
    }

    func testTimeZonesComeFromTheContract() {
        XCTAssertEqual(DomainContract.timeZoneNames.first, "UTC")
        XCTAssertTrue(DomainContract.timeZoneNames.contains("Europe/Vienna"))
    }

    // MARK: - The draft (web `ScheduleDraft` helpers)

    func testDraftReadsAStoredSchedule() {
        let draft = ScheduleDraft(cron: "0 9 * * 1-5", tz: nil)
        XCTAssertEqual(draft.tz, "UTC")
        XCTAssertEqual(draft.schedule?.repeat, "weekly")
        XCTAssertEqual(draft.wireCron, "0 9 * * 1,2,3,4,5")
        XCTAssertEqual(ScheduleDraft(cron: "0 9 * * *", tz: "  ").tz, "UTC")
        XCTAssertEqual(ScheduleDraft(cron: "0 9 * * *", tz: "Europe/Vienna").tz, "Europe/Vienna")
    }

    func testFreshDraftIsDailyAtNine() {
        let draft = ScheduleDraft.fresh(tz: "Europe/Vienna")
        XCTAssertEqual(draft.schedule, SchedulePicker.defaults())
        XCTAssertEqual(draft.wireCron, "0 9 * * *")
        XCTAssertEqual(draft.tz, "Europe/Vienna")
        XCTAssertEqual(draft.errors, [])
    }

    func testCustomDraftKeepsItsExpressionAndValidatesFiveFields() {
        var draft = ScheduleDraft(cron: "*/10   * * * 1-5", tz: "UTC")
        XCTAssertNil(draft.schedule)
        XCTAssertEqual(draft.wireCron, "*/10 * * * 1-5")
        XCTAssertEqual(draft.errors, [])
        draft.cron = "*/10 * *"
        XCTAssertEqual(draft.errors, [SchedulePicker.Copy.errorCron])
    }

    func testPickRepeatSeedsAndReplacesCustom() {
        var custom = ScheduleDraft(cron: "*/10 * * * 1-5", tz: "UTC")
        custom.pick(repeat: "monthly")
        XCTAssertEqual(custom.schedule?.dayOfMonth, 1)
        XCTAssertEqual(custom.cron, "")
        XCTAssertEqual(custom.wireCron, "0 9 1 * *")

        var daily = ScheduleDraft(cron: "30 7 * * *", tz: "UTC")
        daily.pick(repeat: "weekly")
        XCTAssertEqual(daily.schedule?.weekdays, [1, 2, 3, 4, 5])
        XCTAssertEqual(daily.wireCron, "30 7 * * 1,2,3,4,5")

        daily.schedule?.weekdays = []
        XCTAssertEqual(daily.errors, [SchedulePicker.Copy.errorNoDays])
        daily.pick(repeat: "hourly")
        XCTAssertEqual(daily.wireCron, "30 * * * *")
        XCTAssertFalse(SchedulePicker.showsTimeZone(daily.schedule))
        daily.pick(repeat: "minutes")
        XCTAssertEqual(daily.wireCron, "*/10 * * * *")
        XCTAssertFalse(SchedulePicker.showsTimeZone(daily.schedule))
        daily.schedule?.every = 5
        XCTAssertEqual(daily.wireCron, "*/5 * * * *")
        daily.pick(repeat: "daily")
        XCTAssertTrue(SchedulePicker.showsTimeZone(daily.schedule))
        XCTAssertTrue(SchedulePicker.showsTimeZone(nil))
    }
}
