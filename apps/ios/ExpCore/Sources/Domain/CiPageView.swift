import Foundation

/// #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json):
/// THE CI page as ONE genUI tree, painted by `ExponentialSurface` like web's
/// `<ExponentialSurface>`. A line-for-line port of web
/// `apps/web/src/lib/ci-page-view.ts`: the same node ids, props and child
/// order (`CiPageViewTests` replays the fixture). Every rule comes from
/// `CiPage`; this only lays the page out. The platform adds its own chrome.
public enum CiPageView {
    /// Events the page's tree fires. Context keys: `run`, `pr`, `device`;
    /// the pickers and tabs carry the control's own `value`.
    public enum Events {
        public static let tab = "ci-tab"
        public static let status = "ci-filter-status"
        public static let repository = "ci-filter-repo"
        public static let source = "ci-filter-source"
        public static let openRun = "ci-open-run"
        public static let openPr = "ci-open-pr"
        public static let openRunner = "ci-open-runner"
        public static let runners = "ci-open-runners"
    }

    public struct State: Equatable, Sendable {
        public var tab: CiPage.Tab
        public var filter: CiPage.RunFilter
        public var phone: Bool

        public init(tab: CiPage.Tab = .runs, filter: CiPage.RunFilter = .all, phone: Bool) {
            self.tab = tab
            self.filter = filter
            self.phone = phone
        }
    }

    private static func trainTone(_ glyph: PrChecks.Glyph) -> String {
        switch glyph {
        case .running: CiPage.statusTone(.running).tone
        case .queued, .skipped: "neutral"
        case .failed: "danger"
        case .passed: "success"
        }
    }

    private static func node(
        _ id: String,
        _ component: String,
        props: [String: CardJSON]? = nil,
        children: [CardJSON]? = nil,
        slots: [String: CardJSON]? = nil,
        on: (event: String, name: String, context: [String: CardJSON]?)? = nil
    ) -> CardJSON {
        var out: [String: CardJSON] = ["id": .string(id), "component": .string(component)]
        if let props { out["props"] = .object(props) }
        if let children { out["children"] = .array(children) }
        if let slots { out["slots"] = .object(slots) }
        if let on {
            var inner: [String: CardJSON] = ["name": .string(on.name)]
            if let context = on.context { inner["context"] = .object(context) }
            out["on"] = .object([on.event: .object(["event": .object(inner)])])
        }
        return .object(out)
    }

    private static func stack(_ id: String, _ direction: String, _ gap: String, _ children: [CardJSON]) -> CardJSON {
        node(id, "Stack", props: ["direction": .string(direction), "gap": .string(gap), "align": .string(direction == "horizontal" ? "center" : "stretch")], children: children)
    }

    private static func trainSection(_ group: CiPage.TrainGroup, _ id: String) -> CardJSON {
        let ids = CiPage.TestIds.self
        return node(
            id, "Section",
            props: [
                "title": .string(group.title),
                "caption": .string(group.caption),
                "icon": .string("pr-batch"),
            ],
            children: group.rows.map { row in
                var props: [String: CardJSON] = ["identifier": .string(row.identifier), "title": .string(row.title), "pressable": .bool(true)]
                if let subtitle = row.subtitle, !subtitle.isEmpty { props["subtitle"] = .string(subtitle) }
                return node(
                    "\(ids.trainRow).\(row.id)", "Row", props: props,
                    slots: [
                        "leading": node("\(ids.trainRow).\(row.id).glyph", "Icon", props: [
                            "name": .string(row.glyph.rawValue), "tone": .string(trainTone(row.glyph)), "size": .string("sm"),
                        ]),
                        "trailing": node("\(ids.trainRow).\(row.id).badge", "Badge", props: [
                            "text": .string(row.badge), "variant": .string("secondary"),
                        ]),
                    ],
                    on: ("press", Events.openPr, ["pr": .string(row.pullRequestId)])
                )
            }
        )
    }

    private static func option(_ label: String, _ value: String) -> CardJSON {
        .object(["label": .string(label), "value": .string(value)])
    }

    /// The filter bar (#EXP-2169 "one look"): ONE Toolbar of three
    /// `Select{button}` pickers (status, repository, source); wide adds the
    /// runner Chip at its end.
    private static func filters(_ input: CiPage.Input, _ state: State) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        func picker(_ id: String, _ label: String, _ value: String, _ options: [CardJSON], _ event: String) -> CardJSON {
            node(id, "Select", props: [
                "name": .string(id),
                "label": .string(label),
                "triggerVariant": .string("button"),
                "value": .string(value),
                "options": .array(options),
            ], on: ("change", event, nil))
        }
        let status = picker(
            ids.statusFilter, copy.filterLabels["status"] ?? "", state.filter.status.rawValue,
            CiPage.Filter.allCases.map { option(copy.filters[$0.rawValue] ?? $0.rawValue, $0.rawValue) }, Events.status
        )
        let repository = picker(
            ids.repoFilter, copy.filterLabels["repository"] ?? "", state.filter.repositoryId ?? "all",
            CiPage.repoOptions(input).map { option($0.label, $0.value) }, Events.repository
        )
        let source = picker(
            ids.sourceFilter, copy.filterLabels["source"] ?? "", state.filter.source?.rawValue ?? "all",
            ["all", "exponential", "github"].map { option(copy.sources[$0] ?? $0, $0) }, Events.source
        )
        let chip = state.phone ? nil : CiPage.runnerChip(input)
        let runners = chip.map { chip in
            node(ids.runnerChip, "Chip", props: [
                "label": .string(chip.label),
                "dot": .string(chip.online ? "$color.success" : "$color.mutedForeground"),
                "pressable": .bool(true),
            ], on: ("press", Events.runners, nil))
        }
        return node(ids.filters, "Toolbar", children: [status, repository, source], slots: runners.map { ["trailing": $0] })
    }

    private static func call(_ name: String, _ args: [String: CardJSON]) -> CardJSON {
        .object(["@call": .string(name), "args": .object(args)])
    }

    /// One run's row. Wide: title, subtitle, trailing duration · age, the
    /// source mark (tooltip) and the author; phones: meta = duration | queued.
    private static func runRow(_ row: CiPage.RunListRow, phone: Bool) -> CardJSON {
        let id = "\(CiPage.TestIds.runRow).\(row.id)"
        let leading = node("\(id).glyph", "Icon", props: [
            "name": .string(row.glyph.rawValue), "tone": .string(row.tone), "size": .string("sm"),
        ])
        let press: (event: String, name: String, context: [String: CardJSON]?) = ("press", Events.openRun, ["run": .string(row.id)])
        if phone {
            return node(id, "Row", props: [
                "title": .string(row.title), "subtitle": .string(row.phoneSubtitle), "meta": .string(row.phoneMeta), "pressable": .bool(true),
            ], slots: ["leading": leading], on: press)
        }
        let age = call("formatRelativeTime", ["value": .string(row.at)])
        let meta = row.duration.map {
            call("fill", ["template": .string("{duration} · {age}"), "params": .object(["duration": .string($0), "age": age])])
        } ?? call("fill", ["template": .string("{queued} · {age}"), "params": .object(["queued": .string(CiPage.Copy.waitingForRunner), "age": age])])
        var trailing: [CardJSON] = [
            node("\(id).meta", "Text", props: ["text": meta, "variant": .string("muted")]),
            node("\(id).source", "Tooltip", props: ["content": .string(row.sourceLabel)], children: [
                node("\(id).source.icon", "Icon", props: [
                    "name": .string(row.sourceIcon), "size": .string("sm"), "tone": .string("neutral"), "label": .string(row.sourceLabel),
                ]),
            ]),
        ]
        if let author = row.author {
            var props: [String: CardJSON] = ["name": .string(author.name), "seed": .string(author.id), "size": .string("sm")]
            if let image = author.image, !image.isEmpty { props["src"] = .string(image) }
            trailing.append(node("\(id).author", "Avatar", props: props))
        }
        return node(id, "Row", props: [
            "title": .string(row.title), "subtitle": .string(row.subtitle), "pressable": .bool(true),
        ], slots: [
            "leading": leading,
            "trailing": stack("\(id).trailing", "horizontal", "sm", trailing),
        ], on: press)
    }

    /// The runs as day Sections (or the EmptyState): the Runs tab's body
    /// and the repo page's Runs tab (`ciRepoRunsTree`).
    private static func runSections(_ input: CiPage.Input, filter: CiPage.RunFilter, phone: Bool, filtered: Bool) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        let groups = CiPage.runGroups(input, filter: filter)
        if groups.isEmpty {
            return node(ids.runs, "EmptyState", props: [
                "icon": .string("nav-ci"), "title": .string(filtered ? copy.Empty.filtered : copy.Empty.runs), "size": .string("list"),
            ])
        }
        return stack(ids.runs, "vertical", "md", groups.map { group in
            var props: [String: CardJSON] = ["title": .string(group.title)]
            if let count = group.count { props["count"] = .number(Double(count)) }
            return node("\(ids.runs).\(group.id)", "Section", props: props, children: group.rows.map { runRow($0, phone: phone) })
        })
    }

    private static func runsPanel(_ input: CiPage.Input, _ state: State) -> CardJSON {
        let filtered = state.filter.status != .all || state.filter.repositoryId != nil || state.filter.source != nil
        return stack("\(CiPage.TestIds.page).runs", "vertical", "md", [
            filters(input, state), runSections(input, filter: state.filter, phone: state.phone, filtered: filtered),
        ])
    }

    /// THE REPO PAGE'S RUNS TAB (#EXP-2169 "one look"): the CI page's runs of
    /// ONE repository, the same day Sections and rows, no filter bar. Events:
    /// the page's `ci-open-run {run}`.
    public static func ciRepoRunsTree(_ input: CiPage.Input, repositoryId: String, phone: Bool) -> CardJSON {
        runSections(input, filter: CiPage.RunFilter(status: .all, repositoryId: repositoryId, source: nil), phone: phone, filtered: false)
    }

    private static func trainPanel(_ input: CiPage.Input) -> CardJSON {
        let ids = CiPage.TestIds.self
        let groups = CiPage.trainGroups(input, queue: true)
        return stack("\(ids.page).train", "vertical", "md", groups.isEmpty
            ? [node(ids.train, "EmptyState", props: [
                "icon": .string("pr-batch"), "title": .string(CiPage.Copy.Empty.train), "size": .string("list"),
            ])]
            : groups.map { trainSection($0, "\(ids.train).\($0.repositoryId)") })
    }

    private static func runnersPanel(_ input: CiPage.Input) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.Runners.self
        let rows = CiPage.runnerRows(input)
        if rows.isEmpty {
            return node(ids.runners, "EmptyState", props: [
                "icon": .string("server"), "title": .string(CiPage.Copy.Empty.runners), "size": .string("list"),
            ])
        }
        return node(ids.runners, "Section", props: [:], children: rows.map { row in
            var props: [String: CardJSON] = [
                "icon": .string("server"), "title": .string(row.title), "subtitle": .string(row.subtitle), "pressable": .bool(true),
            ]
            if let busy = row.busy, !busy.isEmpty {
                props["meta"] = .string(busy)
            } else if let since = row.offlineSince, !since.isEmpty {
                props["meta"] = .object([
                    "@call": .string("fill"),
                    "args": .object([
                        "template": .string(copy.offlineSince),
                        "params": .object(["time": .object([
                            "@call": .string("formatRelativeTime"), "args": .object(["value": .string(since)]),
                        ])]),
                    ]),
                ])
            }
            let slots: [String: CardJSON]? = row.queue.map {
                ["trailing": node("\(ids.runnerRow).\(row.id).queue", "Badge", props: ["text": .string($0), "variant": .string("outline")])]
            }
            return node("\(ids.runnerRow).\(row.id)", "Row", props: props, slots: slots, on: ("press", Events.openRunner, ["device": .string(row.id)]))
        })
    }

    /// The CI page's body (ci-page.json LAYOUT): the CURRENT tab's panel.
    /// The tab switcher is the platform's own segmented control (#EXP-2169
    /// "one look": the board's switcher, counts from `CiPage.tabCounts`),
    /// drawn by the host above this tree.
    public static func ciPageTree(_ input: CiPage.Input, state: State) -> CardJSON {
        switch state.tab {
        case .runs: runsPanel(input, state)
        case .train: trainPanel(input)
        case .runners: runnersPanel(input)
        }
    }

    /// A control event's next state (`ci-tab` and the filters carry the
    /// control's `value`; `all` = no filter; `ci-open-runners` = the Runners
    /// tab); nil = not a state event.
    public static func stateAfter(_ state: State, event: String, payload: [String: CardJSON]?) -> State? {
        var next = state
        if event == Events.runners {
            // The runner Chip opens the Runners tab.
            next.tab = .runners
            return next
        }
        guard case let .string(value)? = payload?["value"] else { return nil }
        switch event {
        case Events.tab:
            guard let tab = CiPage.Tab(rawValue: value) else { return nil }
            next.tab = tab
        case Events.status:
            guard let status = CiPage.Filter(rawValue: value) else { return nil }
            next.filter.status = status
        case Events.repository:
            next.filter.repositoryId = value == "all" || value.isEmpty ? nil : value
        case Events.source:
            next.filter.source = value == "all" ? nil : CiPage.Source(rawValue: value)
        default:
            return nil
        }
        return next
    }
}
