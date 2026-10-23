import Foundation

// NEW-2: THE schedule picker's pure core ×4 — nobody speaks cron. A schedule
// trigger STORES a 5-field cron; the rows (ExpUI `ScheduleFields`) edit it and
// `scheduleSummary` reads it. A 1:1 port of the TS reference
// `packages/domain-contract/src/schedule-picker.ts` (desktop
// `domain::schedule_picker`, Android `domain/SchedulePicker.kt`), byte-locked
// by `fixtures/schedule-picker.json` (`cronCases`, `parseCases`,
// `weekdaysLabelCases`, `summaryCases`, replayed by `SchedulePickerTests`).
// The fixture's `$comment` holds the rules.

/// What the rows edit; `SchedulePicker.cronFromSchedule` is its wire form.
public struct Schedule: Equatable, Sendable {
    /// minutes | hourly | daily | weekly | monthly (`SchedulePicker.repeatValues`).
    public var `repeat`: String
    /// EXP-1363: one of `SchedulePicker.everyValues`; minutes only (a read
    /// carries `defaults.every` otherwise).
    public var every: Int
    /// 0..59; unused by `minutes`.
    public var minute: Int
    /// 0..23; unused by `hourly`.
    public var hour: Int
    /// ISO weekdays 1 = Monday .. 7 = Sunday, ascending, unique; weekly only.
    public var weekdays: [Int]
    /// 1..28; monthly only.
    public var dayOfMonth: Int

    public init(repeat: String, every: Int = SchedulePicker.defaultEvery, minute: Int, hour: Int, weekdays: [Int], dayOfMonth: Int) {
        self.repeat = `repeat`
        self.every = every
        self.minute = minute
        self.hour = hour
        self.weekdays = weekdays
        self.dayOfMonth = dayOfMonth
    }
}

public enum SchedulePicker {
    /// Fixture `copy`.
    public enum Copy {
        public static let rowRepeat = "Repeat"
        public static let rowDays = "Days"
        public static let rowDay = "Day"
        public static let rowMinute = "Minute"
        public static let rowTime = "Time"
        public static let rowTimeZone = "Time zone"
        public static let rowCron = "Cron"
        public static let rowEvery = "Every"

        public static let repeatMinutes = "Every few minutes"

        public static let repeatHourly = "Every hour"
        public static let repeatDaily = "Every day"
        public static let repeatWeekly = "Every week"
        public static let repeatMonthly = "Every month"
        public static let repeatCustom = "Custom"
        /// `copy.repeat` by repeat value (incl. `custom`).
        public static let repeatLabels: [String: String] = [
            "minutes": repeatMinutes,
            "hourly": repeatHourly,
            "daily": repeatDaily,
            "weekly": repeatWeekly,
            "monthly": repeatMonthly,
            "custom": repeatCustom,
        ]

        public static let weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        public static let weekdaysShort = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        public static let weekdaysAll = "Every day"
        public static let weekdaysWork = "Weekdays"
        public static let weekdaysWeekend = "Weekends"
        public static let dayOfMonth = "Day {day}"
        public static let minuteOfHour = ":{minute}"
        public static let timeZonePlaceholder = "Search time zones"
        public static let cronPlaceholder = "0 9 * * 1-5"

        /// `copy.every` (`{n}`): an Every option's label.
        public static let every = "{n} minutes"
        public static let summaryMinutes = "Every {n} minutes"
        public static let summaryHourly = "Every hour at :{minute}"
        public static let summaryDaily = "Every day at {time}"
        public static let summaryWeekly = "{days} at {time}"
        public static let summaryWeeklyOne = "Every {weekday} at {time}"
        public static let summaryMonthly = "Every month on day {day} at {time}"
        public static let summaryCustom = "Cron {cron}"
        public static let summaryZoned = "{summary} ({tz})"

        public static let errorNoDays = "Pick a day"
        public static let errorCron = "Use five fields: minute hour day month weekday"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let `repeat` = "schedule-repeat"
        public static let days = "schedule-days"
        public static let day = "schedule-day"
        public static let minute = "schedule-minute"
        public static let time = "schedule-time"
        public static let timeZone = "schedule-time-zone"
        public static let cron = "schedule-cron"
        public static let every = "schedule-every"
    }

    public static let repeatValues = ["minutes", "hourly", "daily", "weekly", "monthly"]
    /// EXP-1363: the `minutes` repeat's intervals.
    public static let everyValues = [5, 10, 15, 20, 30]
    /// Fixture `defaults.every`.
    public static let defaultEvery = 10
    /// 28 so no month is skipped.
    public static let dayOfMonthMax = 28

    /// A new schedule: daily at 09:00 (the fixture's `defaults`; `weekdays`
    /// and `dayOfMonth` = what weekly / monthly seed).
    public static func defaults() -> Schedule {
        Schedule(repeat: "daily", every: defaultEvery, minute: 0, hour: 9, weekdays: [1, 2, 3, 4, 5], dayOfMonth: 1)
    }

    /// `copy.every` for one interval (an Every option / row value).
    public static func everyLabel(_ every: Int) -> String {
        Copy.every.replacingOccurrences(of: "{n}", with: String(every))
    }

    /// EXP-1363: the Time zone row shows for daily, weekly, monthly and
    /// custom only (nil = custom).
    public static func showsTimeZone(_ schedule: Schedule?) -> Bool {
        guard let schedule else { return true }
        return schedule.repeat != "minutes" && schedule.repeat != "hourly"
    }

    static func pad2(_ value: Int) -> String {
        value < 10 && value >= 0 ? "0\(value)" : String(value)
    }

    /// The 24h `HH:mm` a Time row reads.
    public static func timeLabel(hour: Int, minute: Int) -> String {
        "\(pad2(hour)):\(pad2(minute))"
    }

    /// `copy.dayOfMonth` for one day.
    public static func dayOfMonthLabel(_ day: Int) -> String {
        Copy.dayOfMonth.replacingOccurrences(of: "{day}", with: String(day))
    }

    /// `copy.minuteOfHour` for one minute.
    public static func minuteOfHourLabel(_ minute: Int) -> String {
        Copy.minuteOfHour.replacingOccurrences(of: "{minute}", with: pad2(minute))
    }

    /// ISO weekday (1 = Monday .. 7 = Sunday) → cron's (Sunday = 0).
    private static func toCronWeekday(_ iso: Int) -> Int { iso == 7 ? 0 : iso }
    /// cron weekday (0 or 7 = Sunday) → ISO.
    private static func fromCronWeekday(_ cron: Int) -> Int { cron == 0 ? 7 : cron }

    /// The canonical cron of a schedule (the fixture's `cronCases`).
    public static func cronFromSchedule(_ schedule: Schedule) -> String {
        let minute = String(schedule.minute)
        switch schedule.repeat {
        case "minutes":
            return "*/\(schedule.every) * * * *"
        case "hourly":
            return "\(minute) * * * *"
        case "weekly":
            let days = Set(schedule.weekdays.map(toCronWeekday)).sorted()
            return "\(minute) \(schedule.hour) * * \(days.map(String.init).joined(separator: ","))"
        case "monthly":
            return "\(minute) \(schedule.hour) \(schedule.dayOfMonth) * *"
        default: // daily
            return "\(minute) \(schedule.hour) * * *"
        }
    }

    private static func isAsciiDigits(_ text: Substring) -> Bool {
        !text.isEmpty && text.unicodeScalars.allSatisfy { $0.value >= 48 && $0.value <= 57 }
    }

    /// A single plain number within [min, max], else nil (no lists, no `*`).
    private static func plainNumber(_ field: Substring, _ min: Int, _ max: Int) -> Int? {
        guard field.count <= 2, isAsciiDigits(field), let value = Int(field) else { return nil }
        return value >= min && value <= max ? value : nil
    }

    /// A cron weekday field as ISO weekdays: plain numbers, lists and ranges of
    /// 0..7 (0 and 7 = Sunday), any order; nil for anything else.
    private static func weekdaysOf(_ field: Substring) -> [Int]? {
        var out = Set<Int>()
        for part in field.split(separator: ",", omittingEmptySubsequences: false) {
            let bounds = part.split(separator: "-", omittingEmptySubsequences: false)
            guard bounds.count == 1 || bounds.count == 2,
                  bounds.allSatisfy({ $0.count == 1 && isAsciiDigits($0) }),
                  let lo = Int(bounds[0]),
                  let hi = bounds.count == 2 ? Int(bounds[1]) : lo
            else { return nil }
            if lo > 7 || hi > 7 || lo > hi { return nil }
            for day in lo...hi { out.insert(fromCronWeekday(day)) }
        }
        return out.sorted()
    }

    /// The schedule a cron reads as, or nil when the rows cannot express it
    /// (a CUSTOM schedule; the fixture's `parseCases`).
    public static func scheduleFromCron(_ cron: String) -> Schedule? {
        let fields = cron.split(whereSeparator: \.isWhitespace)
        guard fields.count == 5 else { return nil }
        let (minuteField, hourField, domField, monthField, dowField) =
            (fields[0], fields[1], fields[2], fields[3], fields[4])
        guard monthField == "*" else { return nil }
        if minuteField.hasPrefix("*/") {
            let step = minuteField.dropFirst(2)
            guard step.count <= 2, isAsciiDigits(step), let every = Int(step) else { return nil }
            let plain = hourField == "*" && domField == "*" && dowField == "*"
            return plain && everyValues.contains(every)
                ? Schedule(repeat: "minutes", every: every, minute: 0, hour: 0, weekdays: [], dayOfMonth: 1)
                : nil
        }
        guard let minute = plainNumber(minuteField, 0, 59) else { return nil }
        // Only the repeat's own fields are populated: `weekdays` [] unless
        // weekly, `dayOfMonth` 1 unless monthly (the rows seed `defaults`).
        if hourField == "*" {
            return domField == "*" && dowField == "*"
                ? Schedule(repeat: "hourly", minute: minute, hour: 0, weekdays: [], dayOfMonth: 1)
                : nil
        }
        guard let hour = plainNumber(hourField, 0, 23) else { return nil }
        if domField == "*" && dowField == "*" {
            return Schedule(repeat: "daily", minute: minute, hour: hour, weekdays: [], dayOfMonth: 1)
        }
        if domField == "*" {
            guard let weekdays = weekdaysOf(dowField), !weekdays.isEmpty else { return nil }
            return Schedule(repeat: "weekly", minute: minute, hour: hour, weekdays: weekdays, dayOfMonth: 1)
        }
        guard dowField == "*", let day = plainNumber(domField, 1, dayOfMonthMax) else { return nil }
        return Schedule(repeat: "monthly", minute: minute, hour: hour, weekdays: [], dayOfMonth: day)
    }

    /// The Days row's value and the weekly summary's `{days}`.
    public static func weekdaysLabel(_ weekdays: [Int]) -> String {
        let days = Set(weekdays).sorted()
        if days.isEmpty { return "" }
        if days == [1, 2, 3, 4, 5, 6, 7] { return Copy.weekdaysAll }
        if days == [1, 2, 3, 4, 5] { return Copy.weekdaysWork }
        if days == [6, 7] { return Copy.weekdaysWeekend }
        func name(_ list: [String], _ day: Int) -> String {
            list.indices.contains(day - 1) ? list[day - 1] : ""
        }
        if days.count == 1 { return name(Copy.weekdays, days[0]) }
        return days.map { name(Copy.weekdaysShort, $0) }.joined(separator: ", ")
    }

    /// A cron with its whitespace collapsed (a custom expression's wire form).
    public static func collapse(_ cron: String) -> String {
        cron.split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }

    /// A Triggers row's text for a stored schedule (the fixture's `summaryCases`).
    public static func scheduleSummary(cron: String, tz: String?) -> String {
        let text: String
        if let schedule = scheduleFromCron(cron) {
            let time = timeLabel(hour: schedule.hour, minute: schedule.minute)
            switch schedule.repeat {
            case "minutes":
                // EXP-1363: a zone means nothing for a minutes schedule.
                return Copy.summaryMinutes.replacingOccurrences(of: "{n}", with: String(schedule.every))
            case "hourly":
                // EXP-1363: nor for an hourly one.
                return Copy.summaryHourly.replacingOccurrences(of: "{minute}", with: pad2(schedule.minute))
            case "weekly":
                text = schedule.weekdays.count == 1
                    ? Copy.summaryWeeklyOne
                        .replacingOccurrences(of: "{weekday}", with: weekdaysLabel(schedule.weekdays))
                        .replacingOccurrences(of: "{time}", with: time)
                    : Copy.summaryWeekly
                        .replacingOccurrences(of: "{days}", with: weekdaysLabel(schedule.weekdays))
                        .replacingOccurrences(of: "{time}", with: time)
            case "monthly":
                text = Copy.summaryMonthly
                    .replacingOccurrences(of: "{day}", with: String(schedule.dayOfMonth))
                    .replacingOccurrences(of: "{time}", with: time)
            default:
                text = Copy.summaryDaily.replacingOccurrences(of: "{time}", with: time)
            }
        } else {
            text = Copy.summaryCustom.replacingOccurrences(of: "{cron}", with: collapse(cron))
        }
        guard let tz, !tz.isEmpty else { return text }
        return Copy.summaryZoned
            .replacingOccurrences(of: "{summary}", with: text)
            .replacingOccurrences(of: "{tz}", with: tz)
    }
}

/// What the rows edit: the structured schedule, or — for a cron the rows
/// cannot express — nil with the expression in `cron`; plus the IANA zone
/// (web `ScheduleDraft`).
public struct ScheduleDraft: Equatable, Sendable {
    public var schedule: Schedule?
    /// The raw expression while `schedule` is nil (custom).
    public var cron: String
    /// IANA zone, never blank on a draft.
    public var tz: String

    /// A stored schedule read into the rows (a missing / blank zone = UTC).
    public init(cron: String, tz: String?) {
        schedule = SchedulePicker.scheduleFromCron(cron)
        self.cron = SchedulePicker.collapse(cron)
        let zone = (tz ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        self.tz = zone.isEmpty ? "UTC" : zone
    }

    /// A new schedule: daily at 09:00 in `tz` (the device's zone).
    public static func fresh(tz: String) -> ScheduleDraft {
        var draft = ScheduleDraft(cron: "", tz: tz)
        draft.schedule = SchedulePicker.defaults()
        return draft
    }

    /// The wire cron: the schedule's canonical form, else the custom
    /// expression with its whitespace collapsed.
    public var wireCron: String {
        schedule.map(SchedulePicker.cronFromSchedule) ?? SchedulePicker.collapse(cron)
    }

    /// Inline errors; [] = savable.
    public var errors: [String] {
        if let schedule {
            return schedule.repeat == "weekly" && schedule.weekdays.isEmpty
                ? [SchedulePicker.Copy.errorNoDays] : []
        }
        return cron.split(whereSeparator: \.isWhitespace).count == 5 ? [] : [SchedulePicker.Copy.errorCron]
    }

    /// Pick a Repeat: replaces a custom cron; weekly with no days seeds
    /// Monday–Friday, monthly from custom seeds day 1.
    public mutating func pick(repeat value: String) {
        let seed = SchedulePicker.defaults()
        let base = schedule ?? seed
        var next = base
        next.repeat = value
        if value == "weekly" && base.weekdays.isEmpty { next.weekdays = seed.weekdays }
        if value == "monthly" && schedule == nil { next.dayOfMonth = seed.dayOfMonth }
        schedule = next
        cron = ""
    }
}
