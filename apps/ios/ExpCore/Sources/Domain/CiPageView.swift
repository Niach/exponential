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

    private static func path(_ key: String) -> CardJSON { .object(["@path": .string(key)]) }

    private static func trainSection(_ group: CiPage.TrainGroup, _ id: String) -> CardJSON {
        let ids = CiPage.TestIds.self
        return node(
            id, "Section",
            props: [
                "title": .string(CiPage.Copy.trainSection),
                "caption": .string("\(group.title) · \(group.caption)"),
                "icon": .string("pr-batch"),
            ],
            children: group.rows.map { row in
                var props: [String: CardJSON] = ["identifier": .string(row.identifier), "title": .string(row.title), "pressable": .bool(true)]
                if let subtitle = row.subtitle, !subtitle.isEmpty { props["subtitle"] = .string(subtitle) }
                return node(
                    "\(ids.trainRow).\(row.id)", "Row", props: props,
                    slots: [
                        "leading": node("\(ids.trainRow).\(row.id).glyph", "Icon", props: [
                            "name": .string(row.glyph.rawValue), "tone": .string("neutral"), "size": .string("sm"),
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

    private static func filters(_ input: CiPage.Input, _ state: State) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        var statusProps: [String: CardJSON] = [
            "items": .array(CiPage.Filter.allCases.map { option(copy.filters[$0.rawValue] ?? $0.rawValue, $0.rawValue) }),
            "value": .string(state.filter.status.rawValue),
            "size": .string("sm"),
        ]
        if state.phone { statusProps["fill"] = .bool(true) }
        let status = node(ids.statusFilter, "Segmented", props: statusProps, on: ("change", Events.status, nil))
        let repository = node(ids.repoFilter, "Select", props: [
            "name": .string(ids.repoFilter),
            "triggerVariant": .string("pill"),
            "value": .string(state.filter.repositoryId ?? "all"),
            "options": .array(CiPage.repoOptions(input).map { option($0.label, $0.value) }),
        ], on: ("change", Events.repository, nil))
        let source = node(ids.sourceFilter, "Select", props: [
            "name": .string(ids.sourceFilter),
            "triggerVariant": .string("pill"),
            "value": .string(state.filter.source?.rawValue ?? "all"),
            "options": .array(["all", "exponential", "github"].map { option(copy.sources[$0] ?? $0, $0) }),
        ], on: ("change", Events.source, nil))
        if state.phone {
            return stack(ids.filters, "vertical", "sm", [
                status, node("\(ids.filters).picks", "Toolbar", children: [repository, source]),
            ])
        }
        return node(ids.filters, "Toolbar", children: [status, repository, source])
    }

    private static func runsPanel(_ input: CiPage.Input, _ state: State) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        let rows = CiPage.runList(input, filter: state.filter)
        var children: [CardJSON] = [filters(input, state)]
        for group in CiPage.trainGroups(input, queue: false) {
            children.append(trainSection(group, "\(ids.train).\(group.repositoryId)"))
        }
        let filtered = state.filter.status != .all || state.filter.repositoryId != nil || state.filter.source != nil
        if rows.isEmpty {
            children.append(node(ids.runs, "EmptyState", props: [
                "icon": .string("nav-ci"), "title": .string(filtered ? copy.Empty.filtered : copy.Empty.runs), "size": .string("list"),
            ]))
        } else if state.phone {
            children.append(node(ids.runs, "Section", props: ["title": .string(copy.runsSection)], children: rows.map { row in
                var props: [String: CardJSON] = ["title": .string(row.title), "subtitle": .string(row.subtitle), "pressable": .bool(true)]
                if let duration = row.duration, !duration.isEmpty { props["meta"] = .string(duration) }
                return node(
                    "\(ids.runRow).\(row.id)", "Row", props: props,
                    slots: ["leading": node("\(ids.runRow).\(row.id).glyph", "Icon", props: [
                        "name": .string(row.glyph.rawValue), "tone": .string(iconTone(row.tone)), "size": .string("sm"),
                    ])],
                    on: ("press", Events.openRun, ["run": .string(row.id)])
                )
            }))
        } else {
            let c = copy.columns
            func column(_ key: String, _ extra: [String: CardJSON] = [:]) -> CardJSON {
                var out: [String: CardJSON] = ["key": .string(key), "label": .string(c[key] ?? key)]
                for (k, v) in extra { out[k] = v }
                return .object(out)
            }
            children.append(node(
                ids.runs, "Table",
                props: [
                    "columns": .array([
                        column("status", ["type": .string("slot"), "slot": .string("status"), "width": .number(120)]),
                        column("run"),
                        column("repository"),
                        column("source", ["type": .string("badge")]),
                        column("ref", ["truncate": .bool(true)]),
                        column("trigger"),
                        column("duration", ["align": .string("end"), "width": .number(96)]),
                        column("started", ["type": .string("relativeTime"), "width": .number(140)]),
                    ]),
                    "rows": .array(rows.map { row in
                        .object([
                            "id": .string(row.id),
                            "status": .string(row.statusLabel),
                            "glyph": .string(row.glyph.rawValue),
                            "tone": .string(iconTone(row.tone)),
                            "run": .string(row.title),
                            "repository": .string(row.repository),
                            "source": .string(row.sourceLabel),
                            "ref": .string(row.ref),
                            "trigger": .string(row.trigger),
                            "duration": .string(row.duration ?? ""),
                            "started": .string(row.startedAt ?? ""),
                        ])
                    }),
                    "rowKey": .string("id"),
                ],
                slots: ["status": stack("\(ids.runs).status", "horizontal", "xs", [
                    node("\(ids.runs).status.glyph", "Icon", props: ["name": path("glyph"), "tone": path("tone"), "size": .string("sm")]),
                    node("\(ids.runs).status.label", "Text", props: ["text": path("status")]),
                ])],
                on: ("rowPress", Events.openRun, ["run": path("id")])
            ))
        }
        return stack("\(ids.page).runs", "vertical", "md", children)
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

    /// The CI page (ci-page.json LAYOUT): the tiles, then the tabs; only the
    /// current tab's panel is built (the others are empty stacks).
    public static func ciPageTree(_ input: CiPage.Input, state: State) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        let counts = CiPage.tabCounts(input)
        func tab(_ value: CiPage.Tab, _ count: Int?) -> CardJSON {
            var out: [String: CardJSON] = ["label": .string(copy.tabs[value.rawValue] ?? value.rawValue), "value": .string(value.rawValue)]
            if let count, count > 0 { out["count"] = .number(Double(count)) }
            return .object(out)
        }
        func panel(_ value: CiPage.Tab) -> CardJSON {
            guard value == state.tab else { return stack("\(ids.page).\(value.rawValue)", "vertical", "md", []) }
            switch value {
            case .runs: return runsPanel(input, state)
            case .train: return trainPanel(input)
            case .runners: return runnersPanel(input)
            }
        }
        var tabsProps: [String: CardJSON] = [
            "tabs": .array([tab(.runs, nil), tab(.train, counts.train), tab(.runners, counts.runners)]),
            "value": .string(state.tab.rawValue),
        ]
        if state.phone { tabsProps["fill"] = .bool(true) }
        return stack(ids.page, "vertical", "lg", [
            node(
                ids.tiles, "Grid",
                props: ["columns": .number(state.phone ? 2 : 4), "gap": .string(state.phone ? "sm" : "md")],
                children: CiPage.tiles(input).enumerated().map { i, tile in
                    var props: [String: CardJSON] = ["label": .string(tile.label), "value": .string(tile.value), "tone": .string(tile.tone)]
                    if let delta = tile.delta, !delta.isEmpty { props["delta"] = .string(delta) }
                    return node("\(ids.tiles).\(i)", "StatTile", props: props)
                }
            ),
            node(ids.tabs, "Tabs", props: tabsProps, children: [panel(.runs), panel(.train), panel(.runners)], on: ("change", Events.tab, nil)),
        ])
    }

    /// A control event's next state (`ci-tab` and the filters carry the
    /// control's `value`; `all` = no filter); nil = not a state event.
    public static func stateAfter(_ state: State, event: String, payload: [String: CardJSON]?) -> State? {
        guard case let .string(value)? = payload?["value"] else { return nil }
        var next = state
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
