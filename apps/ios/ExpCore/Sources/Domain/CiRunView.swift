import Foundation

/// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json):
/// the CI run page and a device's CI runner section as genUI trees (the core
/// catalog's Breadcrumb, Heading, Chip, Toolbar, Button, Menu, Alert, Section,
/// Row, Card, Trace, CodeBlock; SettingsRow, Switch, ChipInput, NumberField,
/// Select; #EXP-2172 the approved redesign), painted by
/// `ExponentialSurface` so iOS draws exactly what the catalogue draws ×4. A
/// line-for-line port of web `apps/web/src/lib/ci-run-view.ts`: the same node
/// ids, props and child order (`CiRunViewTests` replays
/// fixtures/ci-run-view.json).
public enum CiRunView {
    /// Events the page's tree fires (action names).
    public enum Events {
        public static let select = "ci-select-job"
        public static let rerun = "ci-rerun"
        public static let rerunAll = "ci-rerun-all"
        public static let cancel = "ci-cancel"
        public static let fix = "ci-fix"
        public static let openExternal = "ci-open-external"
        public static let breadcrumb = "ci-breadcrumb"
        /// The ⋯ Menu; payload `value` = rerunAll | openExternal.
        public static let menu = "ci-run-menu"
    }

    /// The run page's input (web `CiRunModel`): the synced rows.
    public struct Model: Equatable, Sendable {
        public let run: CiRun.Run
        /// The repository's full name (nil while its row has not synced).
        public let repository: String?
        /// The run's pull request, when known.
        public let pullRequest: CiRun.PullRequest?
        /// The author's name (the users row of createdByUserId), when known.
        public let author: String?
        public let jobs: [CiRun.Job]

        public init(run: CiRun.Run, repository: String?, pullRequest: CiRun.PullRequest?, author: String?, jobs: [CiRun.Job]) {
            self.run = run
            self.repository = repository
            self.pullRequest = pullRequest
            self.author = author
            self.jobs = jobs
        }
    }

    /// ci-run.json `specimen` as a model.
    public static let specimenModel = Model(
        run: CiRun.specimenRun, repository: CiRun.specimenRepository, pullRequest: CiRun.specimenPullRequest,
        author: CiRun.specimenAuthor, jobs: CiRun.specimenJobs
    )

    /// The run's title (ci-page.json's row rule, without the train batch).
    public static func ciRunTitle(_ model: Model) -> String { model.run.title ?? model.run.name }

    /// The platform header on phones: `name #n` over the repository.
    public static func ciRunHeader(_ model: Model) -> (title: String, subtitle: String?) {
        ("\(model.run.name) #\(model.run.number)", model.repository)
    }

    /// Events the runner section fires.
    public enum RunnerEvents {
        public static let enabled = "runner-enabled"
        public static let labels = "runner-labels"
        public static let slots = "runner-slots"
        public static let timeout = "runner-timeout"
    }

    /// The pointer the ChipInput's `values` is bound to (the host writes the
    /// normalized list back, so a refused entry leaves the field).
    public static let runnerLabelsPath = "/labels"

    /// Which button's call is in flight.
    public enum Busy: String, Sendable { case rerun, rerunAll, cancel }

    private static func iconTone(_ tone: PrChecks.Tone) -> String {
        switch tone {
        case .muted: "neutral"
        case .green: "success"
        case .red: "danger"
        }
    }

    private static func node(
        _ id: String,
        _ component: String,
        props: [String: CardJSON]? = nil,
        children: [CardJSON]? = nil,
        extra: [String: CardJSON] = [:]
    ) -> CardJSON {
        var out: [String: CardJSON] = ["id": .string(id), "component": .string(component)]
        if let props { out["props"] = .object(props) }
        for (key, value) in extra { out[key] = value }
        if let children { out["children"] = .array(children) }
        return .object(out)
    }

    private static func on(_ event: String, _ name: String, context: [String: CardJSON]? = nil) -> [String: CardJSON] {
        var inner: [String: CardJSON] = ["name": .string(name)]
        if let context { inner["context"] = .object(context) }
        return ["on": .object([event: .object(["event": .object(inner)])])]
    }

    private static func stack(_ id: String, _ direction: String, _ gap: String, _ align: String, _ children: [CardJSON]) -> CardJSON {
        node(id, "Stack", props: ["direction": .string(direction), "gap": .string(gap), "align": .string(align)], children: children)
    }

    /// A job row's leading glyph, tinted by its tone.
    private static func glyphNode(_ id: String, _ row: CiRun.JobRow) -> CardJSON {
        node(id, "Icon", props: ["name": .string(row.glyph.rawValue), "tone": .string(iconTone(row.tone)), "size": .string("sm")])
    }

    private static func call(_ name: String, _ args: [String: CardJSON]) -> CardJSON {
        .object(["@call": .string(name), "args": .object(args)])
    }

    private static func pageStack(_ children: [CardJSON]) -> CardJSON {
        stack(CiRun.TestIds.page, "vertical", "lg", "stretch", children)
    }

    /// One job's Card: its summary line, the steps with the open step holding
    /// the log (or the log alone when the job reported no steps).
    private static func jobCard(_ model: Model, _ jobId: String, _ log: String?, _ now: Date, _ phone: Bool) -> CardJSON {
        let ids = CiRun.TestIds.self
        guard let job = model.jobs.first(where: { $0.id == jobId }) else { return node("\(ids.jobCard).\(jobId)", "Card") }
        let steps = CiRun.steps(job.steps, now: now)
        let view = CiRun.logView(log ?? "")
        let running = job.status != "completed"
        let logNode: CardJSON = view.code.isEmpty && job.status != "in_progress"
            ? node("\(ids.log).\(job.id)", "Text", props: ["text": .string(CiRun.Copy.emptyLog), "variant": .string("muted")])
            : node("\(ids.log).\(job.id)", "CodeBlock", props: [
                "code": .string(view.code),
                "title": .string(CiRun.Copy.log),
                "language": .string("ansi"),
                "lineNumbers": .bool(true),
                "wrap": .bool(false),
                "copyable": .bool(true),
                "maxLines": .number(Double(phone ? CiRun.logMaxLinesPhone : CiRun.logMaxLinesWide)),
                "highlight": .array(view.highlight.map { .number(Double($0)) }),
                "follow": .bool(running),
            ])
        let openAt = steps.firstIndex { $0.status == .error }
            ?? steps.firstIndex { $0.status == .running }
            ?? steps.count - 1
        let children: [CardJSON] = steps.isEmpty
            ? [logNode]
            : [node(
                "\(ids.steps).\(job.id)", "Trace", props: ["size": .string("sm")],
                children: steps.enumerated().map { i, step in
                    var props: [String: CardJSON] = ["title": .string(step.title), "status": .string(step.status.rawValue)]
                    if let description = step.description, !description.isEmpty { props["description"] = .string(description) }
                    props["collapsible"] = .bool(true)
                    props["open"] = .bool(i == openAt)
                    return node("\(ids.steps).\(job.id).\(i)", "TraceItem", props: props, children: i == openAt ? [logNode] : nil)
                }
            )]
        return node(
            "\(ids.jobCard).\(job.id)", "Card",
            props: ["title": .string(job.check), "description": .string(CiRun.jobSummary(job, now: now))],
            children: children
        )
    }

    /// The run page (ci-run.json LAYOUT). `log` = the SELECTED job's log text
    /// as read so far (nil = not read yet). Phones: `jobOpen` = drilled into
    /// the selected job (the tree is its Card alone).
    public static func ciRunTree(
        _ model: Model,
        selected requested: String?,
        log: String?,
        now: Date,
        phone: Bool,
        busy: Busy? = nil,
        jobOpen: Bool = false
    ) -> CardJSON {
        let ids = CiRun.TestIds.self
        let run = model.run
        let selected = requested ?? CiRun.defaultJob(model.jobs, requested: nil)
        if phone, jobOpen, let selected {
            return pageStack([jobCard(model, selected, log, now, phone)])
        }
        let title = CiRun.Copy.title(run.number)
        var head: [CardJSON] = []
        if !phone {
            var items: [CardJSON] = [.object(["label": .string(CiRun.Copy.breadcrumbRoot), "value": .string("ci")])]
            if let repository = model.repository { items.append(.object(["label": .string(repository), "value": .string("repository")])) }
            items.append(.object(["label": .string(title), "value": .string("run")]))
            head.append(node(ids.breadcrumb, "Breadcrumb", props: ["items": .array(items)], extra: on("select", Events.breadcrumb)))
        }
        head.append(node("ci-run-heading", "Heading", props: ["text": .string(ciRunTitle(model)), "level": .string(phone ? "h3" : "h2")]))
        if !phone {
            let age = call("formatRelativeTime", ["value": .string(run.startedAt ?? run.createdAt ?? "")])
            head.append(node("ci-run-name", "Text", props: [
                "text": call("fill", [
                    "template": .string("{workflow} · {repository}{age}"),
                    "params": .object([
                        "workflow": .string("\(run.name) #\(run.number)"),
                        "repository": .string(model.repository ?? ""),
                        "age": call("fill", ["template": .string(" · {t}"), "params": .object(["t": age])]),
                    ]),
                ]),
                "variant": .string("muted"),
            ]))
        }
        let chips = node(
            ids.chips, "Stack",
            props: ["direction": .string("horizontal"), "gap": .string("xs"), "align": .string("center"), "wrap": .bool(true)],
            children: CiRun.chips(run: run, jobs: model.jobs, pullRequest: model.pullRequest, author: model.author, now: now).map { chip in
                var props: [String: CardJSON] = ["label": .string(chip.label)]
                if let icon = chip.icon { props["icon"] = .string(icon) }
                if let dot = chip.dot { props["dot"] = .string(dot) }
                return node("\(ids.chips).\(chip.id)", "Chip", props: props)
            }
        )
        let actions = CiRun.actions(run)
        func button(_ id: String, _ label: String, _ event: String, _ key: Busy?, _ extra: [String: CardJSON] = [:]) -> CardJSON {
            var props: [String: CardJSON] = ["label": .string(label), "size": .string("sm")]
            for (k, v) in extra { props[k] = v }
            if let key { props["loading"] = .bool(busy == key) }
            props["disabled"] = .bool(busy != nil)
            return node(id, "Button", props: props, extra: on("press", event))
        }
        var buttons: [CardJSON] = []
        switch actions.primary {
        case .cancel?:
            buttons.append(button(ids.cancel, CiRun.Copy.cancel, Events.cancel, .cancel, ["icon": .string("circle-stop")]))
        case .rerunFailed?:
            buttons.append(button(ids.rerun, CiRun.Copy.rerunFailed, Events.rerun, .rerun, ["icon": .string("refresh-cw")]))
        case .rerunAll?:
            buttons.append(button(ids.rerunAll, CiRun.Copy.rerunAll, Events.rerunAll, .rerunAll, ["icon": .string("refresh-cw")]))
        case nil:
            break
        }
        if actions.fix {
            buttons.append(button(ids.fix, CiRun.Copy.fix, Events.fix, nil, ["variant": .string("outline"), "icon": .string("bot")]))
        }
        if !actions.menu.isEmpty {
            let items: [CardJSON] = actions.menu.map { item in
                .object([
                    "label": .string(item == .rerunAll ? CiRun.Copy.rerunAll : CiRun.Copy.openExternal),
                    "value": .string(item.rawValue),
                ])
            }
            buttons.append(node(
                ids.more, "Menu",
                props: ["label": .string(CiRun.Copy.more), "align": .string("end"), "items": .array(items)],
                children: [
                    node("\(ids.more).button", "Button", props: [
                        "icon": .string("ellipsis"), "variant": .string("ghost"), "size": .string("icon"),
                        "label": .string(CiRun.Copy.more), "disabled": .bool(busy != nil),
                    ]),
                ],
                extra: on("select", Events.menu)
            ))
        }
        let actionsNode = stack("ci-run-actions", "horizontal", "sm", "center", buttons)
        var children: [CardJSON] = [
            stack("ci-run-title", "vertical", "xs", "stretch", head),
            phone
                ? stack("ci-run-bar", "vertical", "sm", "stretch", [chips] + (buttons.isEmpty ? [] : [actionsNode]))
                : node("ci-run-bar", "Toolbar", children: [chips], extra: buttons.isEmpty ? [:] : ["slots": .object(["trailing": actionsNode])]),
        ]
        if let error = run.error {
            children.append(node("ci-run-error", "Alert", props: [
                "type": .string("error"), "title": .string(CiRun.Copy.pipelineError), "message": .string(error),
            ]))
            return pageStack(children)
        }
        let failedJob = model.jobs.first { $0.id == selected && CiRun.jobStatus($0) == .failed }
        if let failure = CiRun.failureSummary(check: failedJob?.check, log: failedJob == nil ? nil : log) {
            children.append(node(ids.failure, "Alert", props: [
                "type": .string("error"), "title": .string(failure.title), "message": .string(failure.message),
            ]))
        }
        if model.jobs.isEmpty { return pageStack(children) }
        let rows = CiRun.jobRows(model.jobs, now: now)
        let section = node(
            "ci-run-jobs-section", "Section",
            props: ["title": .string(CiRun.Copy.jobs), "caption": .string(CiRun.jobsCaption(model.jobs))],
            children: rows.map { row in
                var props: [String: CardJSON] = ["title": .string(row.title)]
                if let subtitle = row.subtitle, !subtitle.isEmpty, !phone { props["subtitle"] = .string(subtitle) }
                if let duration = row.duration, !duration.isEmpty {
                    props["meta"] = .string(duration)
                } else if let subtitle = row.subtitle, !subtitle.isEmpty, phone {
                    props["meta"] = .string(subtitle)
                }
                props["selected"] = .bool(row.id == selected)
                props["pressable"] = .bool(true)
                if phone { props["chevron"] = .string("right") }
                var extra = on("press", Events.select, context: ["job": .string(row.id)])
                extra["slots"] = .object(["leading": node("\(ids.jobRow).\(row.id).glyph", "Icon", props: [
                    "name": .string(row.glyph.rawValue), "tone": .string(row.statusTone ?? "neutral"), "size": .string("sm"),
                ])])
                return node("\(ids.jobRow).\(row.id)", "Row", props: props, extra: extra)
            }
        )
        if phone {
            children.append(section)
        } else {
            var grid: [CardJSON] = [
                node("ci-run-jobs-list", "Box", children: [section], extra: ["style": .object(["flexDirection": .string("column")])]),
            ]
            if let selected {
                grid.append(node(
                    "ci-run-detail", "Box",
                    children: [jobCard(model, selected, log, now, phone)],
                    extra: ["style": .object(["minWidth": .number(0), "flexDirection": .string("column")])]
                ))
            }
            children.append(node("ci-run-jobs", "Box", children: grid, extra: ["style": .object([
                "display": .string("grid"),
                "gridTemplateColumns": .string("300px minmax(0, 1fr)"),
                "gap": .string("$spacing.lg"),
                "alignItems": .string("start"),
            ])]))
        }
        return pageStack(children)
    }

    // MARK: - The CI runner section (runner-settings.json)

    /// A ChipInput's entries as stored: each through `normalizeLabel`,
    /// duplicates dropped, at most `limits.labels`; `refused` = an entry the
    /// rule turned down (shown inline).
    public static func runnerLabelsFrom(_ entries: [String]) -> (labels: [String], refused: Bool) {
        var labels: [String] = []
        var refused = false
        for entry in entries {
            if let label = RunnerSettings.normalizeLabel(entry) {
                if !labels.contains(label), labels.count < RunnerSettings.maxLabels { labels.append(label) }
            } else {
                refused = true
            }
        }
        return (labels, refused)
    }

    /// The section (runner-settings.json LAYOUT). `client` = the notice's client.
    public static func runnerSectionTree(
        client: String,
        caps: [String],
        platform: String?,
        settings: RunnerSettings.Settings,
        owner: Bool,
        refused: Bool,
        recent: [CiRun.JobRow]
    ) -> CardJSON {
        let copy = RunnerSettings.Copy.self
        let ids = RunnerSettings.TestIds.self
        let disabled = CardJSON.bool(!owner)
        let os = RunnerSettings.osLabel(platform)
        var children: [CardJSON] = []
        if let key = RunnerSettings.notice(client: client, caps: caps), let n = RunnerSettings.notices[key] {
            children.append(node("runner-notice", "Alert", props: [
                "type": .string(n.type), "title": .string(n.title), "message": .string(n.message),
            ]))
        }
        children.append(node(
            "runner-enabled-row", "SettingsRow",
            props: ["label": .string(copy.enabledLabel), "description": .string(copy.enabledDescription)],
            children: [
                node(ids.toggle, "Switch", props: [
                    "label": .string(""), "name": .string(ids.toggle), "checked": .bool(settings.enabled), "disabled": disabled,
                ], extra: on("change", RunnerEvents.enabled)),
            ]
        ))
        if settings.enabled {
            children.append(node(ids.labels, "ChipInput", props: [
                "label": .string(copy.labelsLabel),
                "name": .string(ids.labels),
                "values": .object(["@path": .string(runnerLabelsPath)]),
                "placeholder": .string(copy.labelsPlaceholder),
                "max": .number(Double(RunnerSettings.maxLabels)),
                "suggestions": .array(os.map { [.object(["label": .string($0), "value": .string($0)])] } ?? []),
                "disabled": disabled,
            ], extra: on("change", RunnerEvents.labels)))
            if refused {
                children.append(node("runner-labels-invalid", "Text", props: [
                    "text": .string(copy.labelsInvalid), "variant": .string("caption"), "tone": .string("danger"),
                ]))
            }
            children.append(node(ids.slots, "NumberField", props: [
                "label": .string(copy.slots),
                "name": .string(ids.slots),
                "value": .number(Double(settings.slots)),
                "min": .number(Double(RunnerSettings.minSlots)),
                "max": .number(Double(RunnerSettings.maxSlots)),
                "step": .number(1),
                "disabled": disabled,
            ], extra: on("change", RunnerEvents.slots)))
            children.append(node(ids.timeout, "Select", props: [
                "label": .string(copy.timeout),
                "name": .string(ids.timeout),
                "options": .array(RunnerSettings.timeoutOptions.map { .object(["value": .string($0.value), "label": .string($0.label)]) }),
                "value": .string(String(settings.timeoutMin)),
                "disabled": disabled,
            ], extra: on("change", RunnerEvents.timeout)))
        }
        children.append(node("runner-recent-heading", "Heading", props: ["text": .string(copy.recent), "level": .string("h4")]))
        if recent.isEmpty {
            children.append(node(ids.recent, "Text", props: ["text": .string(copy.noJobs), "variant": .string("muted")]))
        } else {
            children.append(stack(ids.recent, "vertical", "none", "stretch", recent.map { row in
                var props: [String: CardJSON] = ["title": .string(row.title)]
                if let subtitle = row.subtitle { props["subtitle"] = .string(subtitle) }
                if let duration = row.duration { props["meta"] = .string(duration) }
                return node("\(ids.recent).\(row.id)", "Row", props: props, extra: [
                    "slots": .object(["leading": glyphNode("\(ids.recent).\(row.id).glyph", row)]),
                ])
            }))
        }
        return node(ids.section, "Section", props: ["title": .string(copy.section)], children: children)
    }

    /// The settings a change event writes (nil = nothing changed / refused)
    /// and whether a label entry was refused.
    public static func runnerSettingsAfter(
        _ current: RunnerSettings.Settings,
        event: String,
        payload: [String: CardJSON]?
    ) -> (settings: RunnerSettings.Settings?, refused: Bool) {
        let p = payload ?? [:]
        switch event {
        case RunnerEvents.enabled:
            if case let .bool(checked)? = p["checked"] {
                var next = current
                next.enabled = checked
                return (checked == current.enabled ? nil : next, false)
            }
        case RunnerEvents.labels:
            if case let .array(values)? = p["values"] {
                let entries = values.map { value -> String in
                    if case let .string(text) = value { return text }
                    return value.compactString
                }
                let (labels, refused) = runnerLabelsFrom(entries)
                var next = current
                next.labels = labels
                return (labels == current.labels ? nil : next, refused)
            }
        case RunnerEvents.slots:
            if case let .number(value)? = p["value"], value.isFinite {
                let slots = min(RunnerSettings.maxSlots, max(RunnerSettings.minSlots, Int((value + 0.5).rounded(.down))))
                var next = current
                next.slots = slots
                return (slots == current.slots ? nil : next, false)
            }
        case RunnerEvents.timeout:
            if case let .string(value)? = p["value"] {
                let known = RunnerSettings.timeoutOptions.contains { $0.value == value }
                guard known, let minutes = Int(value), minutes != current.timeoutMin else { return (nil, false) }
                var next = current
                next.timeoutMin = minutes
                return (next, false)
            }
        default:
            break
        }
        return (nil, false)
    }
}
