import Foundation

/// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json):
/// the CI run page and a device's CI runner section as genUI trees (the core
/// catalog's StatTile, Section, Row, Collapsible, Trace, CodeBlock, Alert,
/// SettingsRow, Switch, ChipInput, NumberField, Select), painted by
/// `ExponentialSurface` so iOS draws exactly what the catalogue draws ×4. A
/// line-for-line port of web `apps/web/src/lib/ci-run-view.ts`: the same node
/// ids, props and child order (`CiRunViewTests` replays
/// fixtures/ci-run-view.json).
public enum CiRunView {
    /// Events the page's tree fires (action names).
    public enum Events {
        public static let select = "ci-select-job"
        public static let rerun = "ci-rerun"
        public static let cancel = "ci-cancel"
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
    public enum Busy: String, Sendable { case rerun, cancel }

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

    /// One job's detail: its steps as a Trace, then its log.
    private static func jobDetail(_ jobs: [CiRun.Job], _ jobId: String, _ log: String?, _ now: Date, _ phone: Bool) -> CardJSON {
        let ids = CiRun.TestIds.self
        guard let job = jobs.first(where: { $0.id == jobId }) else { return stack("ci-run-detail.\(jobId)", "vertical", "md", "stretch", []) }
        let steps = CiRun.steps(job.steps, now: now)
        let view = CiRun.logView(log ?? "")
        let running = job.status != "completed"
        var children: [CardJSON] = []
        if !steps.isEmpty {
            children.append(node(
                "\(ids.steps).\(job.id)", "Trace",
                children: steps.enumerated().map { i, step in
                    var props: [String: CardJSON] = ["title": .string(step.title), "status": .string(step.status.rawValue)]
                    if let description = step.description, !description.isEmpty { props["description"] = .string(description) }
                    return node("\(ids.steps).\(job.id).\(i)", "TraceItem", props: props)
                }
            ))
        }
        if view.code.isEmpty, job.status != "in_progress" {
            children.append(node("\(ids.log).\(job.id)", "Text", props: ["text": .string(CiRun.Copy.emptyLog), "variant": .string("muted")]))
        } else {
            children.append(node("\(ids.log).\(job.id)", "CodeBlock", props: [
                "code": .string(view.code),
                "title": .string(CiRun.Copy.log),
                "language": .string("ansi"),
                "lineNumbers": .bool(true),
                "wrap": .bool(false),
                "copyable": .bool(true),
                "maxLines": .number(Double(phone ? CiRun.logMaxLinesPhone : CiRun.logMaxLinesWide)),
                "highlight": .array(view.highlight.map { .number(Double($0)) }),
                "follow": .bool(running),
            ]))
        }
        return stack("ci-run-detail.\(job.id)", "vertical", "md", "stretch", children)
    }

    /// The run page (ci-run.json LAYOUT). `log` = the SELECTED job's log text
    /// as read so far (nil = not read yet).
    public static func ciRunTree(
        run: CiRun.Run,
        jobs: [CiRun.Job],
        selected: String?,
        log: String?,
        now: Date,
        phone: Bool,
        busy: Busy? = nil
    ) -> CardJSON {
        let ids = CiRun.TestIds.self
        let tiles = CiRun.tiles(run, jobs: jobs, now: now)
        let actions = CiRun.actions(state: run.state, error: run.error)
        var children: [CardJSON] = [
            stack("ci-run-title", "vertical", "xs", "stretch", [
                node("ci-run-heading", "Heading", props: ["text": .string(CiRun.Copy.title(run.number)), "level": .string("h2")]),
                node("ci-run-name", "Text", props: ["text": .string(run.name), "variant": .string("muted")]),
            ]),
            node(
                ids.tiles, "Grid",
                props: ["columns": .number(phone ? 2 : 4), "gap": .string("md")],
                children: tiles.enumerated().map { i, tile in
                    var props: [String: CardJSON] = [
                        "label": .string(tile.label), "value": .string(tile.value), "tone": .string(tile.tone.rawValue),
                    ]
                    if let delta = tile.delta { props["delta"] = .string(delta) }
                    return node("\(ids.tiles).\(i)", "StatTile", props: props)
                }
            ),
        ]
        if actions.rerunFailed || actions.cancel {
            var buttons: [CardJSON] = []
            if actions.rerunFailed {
                buttons.append(node(ids.rerun, "Button", props: [
                    "label": .string(CiRun.Copy.rerunFailed),
                    "loading": .bool(busy == .rerun),
                    "disabled": .bool(busy != nil),
                ], extra: on("press", Events.rerun)))
            }
            if actions.cancel {
                // ci-run.json: variant destructive-outline (the catalog's
                // `destructive`; it has no outline twin).
                buttons.append(node(ids.cancel, "Button", props: [
                    "label": .string(CiRun.Copy.cancel),
                    "variant": .string("destructive"),
                    "loading": .bool(busy == .cancel),
                    "disabled": .bool(busy != nil),
                ], extra: on("press", Events.cancel)))
            }
            children.append(stack("ci-run-actions", "horizontal", "sm", "center", buttons))
        }
        if let error = run.error {
            children.append(node("ci-run-error", "Alert", props: [
                "type": .string("error"), "title": .string(CiRun.Copy.pipelineError), "message": .string(error),
            ]))
        } else if !jobs.isEmpty {
            let rows = CiRun.jobRows(jobs, now: now)
            if phone {
                children.append(stack("ci-run-jobs", "vertical", "sm", "stretch", rows.map { row in
                    node(
                        "\(ids.jobRow).\(row.id)", "Collapsible",
                        props: ["title": .string(row.title), "open": .bool(row.id == selected)],
                        children: [jobDetail(jobs, row.id, row.id == selected ? log : nil, now, phone)],
                        extra: on("change", Events.select, context: ["job": .string(row.id)])
                    )
                }))
            } else {
                let list = node(
                    "ci-run-jobs-list", "Box",
                    children: [
                        node(
                            "ci-run-jobs-section", "Section", props: ["title": .string(CiRun.Copy.jobs)],
                            children: rows.map { row in
                                var props: [String: CardJSON] = ["title": .string(row.title)]
                                if let subtitle = row.subtitle { props["subtitle"] = .string(subtitle) }
                                if let duration = row.duration { props["meta"] = .string(duration) }
                                props["selected"] = .bool(row.id == selected)
                                props["pressable"] = .bool(true)
                                var extra = on("press", Events.select, context: ["job": .string(row.id)])
                                extra["slots"] = .object(["leading": glyphNode("\(ids.jobRow).\(row.id).glyph", row)])
                                return node("\(ids.jobRow).\(row.id)", "Row", props: props, extra: extra)
                            }
                        ),
                    ],
                    extra: ["style": .object(["flexDirection": .string("column")])]
                )
                var grid: [CardJSON] = [list]
                if let selected {
                    grid.append(node(
                        "ci-run-detail", "Box",
                        children: [jobDetail(jobs, selected, log, now, phone)],
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
        }
        return stack(ids.page, "vertical", "lg", "stretch", children)
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
