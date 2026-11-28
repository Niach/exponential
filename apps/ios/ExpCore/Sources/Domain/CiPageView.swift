import Foundation

/// #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json):
/// THE CI page as ONE genUI tree, painted by `ExponentialSurface` like web's
/// `<ExponentialSurface>`. A line-for-line port of web
/// `apps/web/src/lib/ci-page-view.ts`: the same node ids, props and child
/// order (`CiPageTests` replays the fixture). #EXP-2214: four tabs, one
/// meaning each: Merge queue, Pull requests, Releases, Runners. Every rule
/// comes from `CiPage`; this only lays the page out. The platform adds its
/// own chrome and the tab switcher.
public enum CiPageView {
    /// Events the page's tree fires. Context keys: `run`, `job`, `pr`,
    /// `device`, `url`; the pickers, tabs and menus carry the control's own
    /// `value`.
    public enum Events {
        public static let tab = "ci-tab"
        public static let status = "ci-filter-status"
        public static let repository = "ci-filter-repo"
        public static let source = "ci-filter-source"
        public static let openRun = "ci-open-run"
        public static let openJob = "ci-open-job"
        public static let openUrl = "ci-open-url"
        public static let openPr = "ci-open-pr"
        public static let openRunner = "ci-open-runner"
        public static let runners = "ci-open-runners"
        /// The Up next ⋯ menu: `value` = merge-now | hold, context `pr`.
        public static let queueMenu = "ci-queue-menu"
        public static let resume = "ci-resume"
    }

    public struct State: Equatable, Sendable {
        public var tab: CiPage.Tab
        public var filter: CiPage.RunFilter
        public var phone: Bool

        public init(tab: CiPage.Tab = .train, filter: CiPage.RunFilter = .all, phone: Bool) {
            self.tab = tab
            self.filter = filter
            self.phone = phone
        }
    }

    /// The repo page's Runs tab's own switch: the CI page's first three tabs.
    public static let repoTabs: [CiPage.Tab] = [.train, .prs, .releases]

    private static let dot: [String: String] = [
        "neutral": "$color.mutedForeground",
        "info": "$color.info",
        "success": "$color.success",
        "danger": "$color.destructive",
        "warning": "$color.warning",
    ]

    private typealias On = (event: String, name: String, context: [String: CardJSON]?)

    private static func node(
        _ id: String,
        _ component: String,
        props: [String: CardJSON]? = nil,
        children: [CardJSON]? = nil,
        slots: [String: CardJSON]? = nil,
        on: On? = nil
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

    private static func vstack(_ id: String, _ children: [CardJSON], gap: String = "md") -> CardJSON {
        node(id, "Stack", props: ["direction": .string("vertical"), "gap": .string(gap), "align": .string("stretch")], children: children)
    }

    private static func hstack(_ id: String, _ children: [CardJSON]) -> CardJSON {
        node(id, "Stack", props: ["direction": .string("horizontal"), "gap": .string("sm"), "align": .string("center")], children: children)
    }

    private static func call(_ name: String, _ args: [String: CardJSON]) -> CardJSON {
        .object(["@call": .string(name), "args": .object(args)])
    }

    private static func relative(_ value: String) -> CardJSON {
        call("formatRelativeTime", ["value": .string(value)])
    }

    private static func option(_ label: String, _ value: String) -> CardJSON {
        .object(["label": .string(label), "value": .string(value)])
    }

    private static func glyph(_ id: String, _ name: String, _ tone: String) -> CardJSON {
        node(id, "Icon", props: ["name": .string(name), "tone": .string(tone), "size": .string("sm")])
    }

    private static func avatar(_ id: String, _ user: CiPage.User) -> CardJSON {
        var props: [String: CardJSON] = ["name": .string(user.name), "seed": .string(user.id), "size": .string("sm")]
        if let image = user.image, !image.isEmpty { props["src"] = .string(image) }
        return node(id, "Avatar", props: props)
    }

    private static func openRun(_ run: String) -> On { ("press", Events.openRun, ["run": .string(run)]) }
    private static func openPr(_ pr: String) -> On { ("press", Events.openPr, ["pr": .string(pr)]) }

    /// The filter bar (#EXP-2169 "one look"): ONE Toolbar of `Select{button}`
    /// pickers, status and repository (#EXP-2214: the source picker is
    /// gone); wide adds the runner Chip at its end.
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
        let chip = state.phone ? nil : CiPage.runnerChip(input)
        let runners = chip.map { chip in
            node(ids.runnerChip, "Chip", props: [
                "label": .string(chip.label),
                "dot": .string(chip.online ? "$color.success" : "$color.mutedForeground"),
                "pressable": .bool(true),
            ], on: ("press", Events.runners, nil))
        }
        return node(ids.filters, "Toolbar", children: [status, repository], slots: runners.map { ["trailing": $0] })
    }

    /// One run's row. Wide: title, subtitle, trailing duration · age, the
    /// source mark (tooltip) and the author; phones: meta = duration | queued.
    private static func runRow(_ row: CiPage.RunListRow, phone: Bool) -> CardJSON {
        let id = "\(CiPage.TestIds.runRow).\(row.id)"
        let leading = glyph("\(id).glyph", row.glyph.rawValue, row.tone)
        if phone {
            return node(id, "Row", props: [
                "title": .string(row.title), "subtitle": .string(row.phoneSubtitle), "meta": .string(row.phoneMeta), "pressable": .bool(true),
            ], slots: ["leading": leading], on: openRun(row.id))
        }
        let age = relative(row.at)
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
        if let author = row.author { trailing.append(avatar("\(id).author", author)) }
        return node(id, "Row", props: [
            "title": .string(row.title), "subtitle": .string(row.subtitle), "pressable": .bool(true),
        ], slots: [
            "leading": leading,
            "trailing": hstack("\(id).trailing", trailing),
        ], on: openRun(row.id))
    }

    /// Runs as day Sections (In progress first), or the EmptyState.
    private static func runSections(
        _ id: String, _ input: CiPage.Input, filter: CiPage.RunFilter, kinds: [CiPage.RunKind], phone: Bool, empty: String
    ) -> CardJSON {
        let groups = CiPage.runGroups(input, filter: filter, kinds: kinds)
        if groups.isEmpty {
            return node(id, "EmptyState", props: ["icon": .string("nav-ci"), "title": .string(empty), "size": .string("list")])
        }
        return vstack(id, groups.map { group in
            var props: [String: CardJSON] = ["title": .string(group.title)]
            if let count = group.count { props["count"] = .number(Double(count)) }
            return node("\(id).\(group.id)", "Section", props: props, children: group.rows.map { runRow($0, phone: phone) })
        })
    }

    private static func filtered(_ filter: CiPage.RunFilter) -> Bool {
        filter.status != .all || filter.repositoryId != nil
    }

    // MARK: - Pull requests

    /// One PR (or branch): its rolled-up status, the deciding run's line
    /// and, wide, the merge queue pill; a press opens that run (#EXP-2214: no
    /// folded runs, genUI's foldable Row drops its leading and trailing slots).
    private static func subjectRow(_ row: CiPage.SubjectRow, phone: Bool) -> CardJSON {
        let id = "\(CiPage.TestIds.prRow).\(row.id)"
        var props: [String: CardJSON] = [
            "title": .string(row.title),
            "subtitle": .string(phone ? row.phoneSubtitle : row.subtitle),
            "pressable": .bool(true),
        ]
        if let identifier = row.identifier { props["identifier"] = .string(identifier) }
        if phone { props["meta"] = .string(row.meta) }
        var slots: [String: CardJSON] = ["leading": glyph("\(id).glyph", row.glyph.rawValue, row.tone)]
        if !phone {
            var trailing = [node("\(id).meta", "Text", props: ["text": .string(row.meta), "variant": .string("muted")])]
            if let pill = row.pill {
                trailing.append(node("\(id).pill", "Chip", props: ["label": .string(pill.text), "dot": .string(dot[pill.tone] ?? "")]))
            }
            slots["trailing"] = hstack("\(id).trailing", trailing)
        }
        return node(id, "Row", props: props, slots: slots, on: openRun(row.runId))
    }

    private static func subjectSections(_ id: String, _ groups: [CiPage.SubjectGroup], phone: Bool, empty: String) -> CardJSON {
        if groups.isEmpty {
            return node(id, "EmptyState", props: ["icon": .string("git-pull-request"), "title": .string(empty), "size": .string("list")])
        }
        return vstack(id, groups.map { group in
            var props: [String: CardJSON] = ["title": .string(group.title), "count": .number(Double(group.count))]
            if !group.open {
                props["collapsible"] = .bool(true)
                props["open"] = .bool(false)
            }
            return node("\(id).\(group.id.rawValue)", "Section", props: props, children: group.rows.map { subjectRow($0, phone: phone) })
        })
    }

    // MARK: - Merge queue

    private static func nowCard(_ now: CiPage.QueueNow, _ id: String) -> CardJSON {
        let copy = CiPage.Copy.Queue.self
        var children: [CardJSON] = [
            now.progress.map { progress in
                var props: [String: CardJSON] = [
                    "value": .number(Double(progress)), "max": .number(100), "label": .string(now.elapsed), "tone": .string("info"),
                ]
                props["valueText"] = now.eta.map(CardJSON.string) ?? .null
                return node("\(id).progress", "Progress", props: props)
            } ?? node("\(id).elapsed", "Text", props: ["text": .string(now.elapsed), "variant": .string("muted")]),
            node("\(id).members", "Section", props: [
                "title": .string(copy.batch), "count": .number(Double(now.members.count)),
            ], children: now.members.map { m in
                node(
                    "\(id).member.\(m.pullRequestId)", "Row",
                    props: ["identifier": .string(m.identifier), "title": .string(m.title), "pressable": .bool(true)],
                    slots: m.author.map { ["trailing": avatar("\(id).member.\(m.pullRequestId).author", $0)] },
                    on: openPr(m.pullRequestId)
                )
            }),
        ]
        if let runId = now.runId {
            children.append(hstack("\(id).actions", [
                node("\(id).open", "Button", props: [
                    "label": .string(copy.openRun), "icon": .string("external-link"), "variant": .string("outline"), "size": .string("sm"),
                ], on: openRun(runId)),
            ]))
        }
        return node(id, "Card", props: ["title": .string(copy.testing), "description": .string(now.subtitle)], children: [vstack("\(id).body", children)])
    }

    private static func queueRow(_ row: CiPage.QueueRow, _ id: String, phone: Bool) -> CardJSON {
        let copy = CiPage.Copy.Queue.self
        var trailing: [CardJSON] = []
        if let at = row.at, !phone { trailing.append(node("\(id).at", "Text", props: ["text": relative(at), "variant": .string("muted")])) }
        if let badge = row.badge {
            trailing.append(node("\(id).badge", "Chip", props: ["label": .string(badge.text), "dot": .string(dot[badge.tone] ?? "")]))
        }
        if row.action == .openLog {
            let press: On = row.jobId.map { job in
                ("press", Events.openJob, ["run": row.runId.map(CardJSON.string) ?? .null, "job": .string(job)])
            } ?? ("press", Events.openUrl, ["url": row.url.map(CardJSON.string) ?? .null])
            trailing.append(node("\(id).log", "Button", props: [
                "label": .string(copy.openLog), "icon": .string("file-text"), "variant": .string("outline"), "size": .string("sm"),
            ], on: press))
        }
        if row.action == .resume {
            trailing.append(node("\(id).resume", "Button", props: [
                "label": .string(copy.resume), "variant": .string("outline"), "size": .string("sm"),
            ], on: ("press", Events.resume, ["pr": .string(row.pullRequestId)])))
        }
        if row.menu {
            trailing.append(node("\(id).menu", "Menu", props: [
                "label": .string(copy.more),
                "items": .array([option(copy.menu["mergeNow"]!, "merge-now"), option(copy.menu["hold"]!, "hold")]),
            ], children: [
                node("\(id).menu.button", "Button", props: [
                    "icon": .string("ellipsis"), "variant": .string("ghost"), "size": .string("icon-xs"), "label": .string(copy.more),
                ]),
            ], on: ("select", Events.queueMenu, ["pr": .string(row.pullRequestId)])))
        }
        var props: [String: CardJSON] = ["identifier": .string(row.identifier), "title": .string(row.title), "pressable": .bool(true)]
        if let subtitle = row.subtitle, !subtitle.isEmpty { props["subtitle"] = .string(subtitle) }
        if let at = row.at, phone, row.action == nil { props["meta"] = relative(at) }
        var slots: [String: CardJSON] = ["leading": glyph("\(id).glyph", row.glyph, row.tone)]
        if !trailing.isEmpty { slots["trailing"] = trailing.count == 1 ? trailing[0] : hstack("\(id).trailing", trailing) }
        return node(id, "Row", props: props, slots: slots, on: openPr(row.pullRequestId))
    }

    private static func queueSection(_ id: String, _ title: String, _ rows: [CiPage.QueueRow], phone: Bool, fold: Bool = false) -> CardJSON? {
        guard !rows.isEmpty else { return nil }
        var props: [String: CardJSON] = ["title": .string(title), "count": .number(Double(rows.count))]
        if fold {
            props["collapsible"] = .bool(true)
            props["open"] = .bool(rows.count <= 5)
        }
        return node(id, "Section", props: props, children: rows.map { queueRow($0, "\(CiPage.TestIds.queueRow).\($0.id)", phone: phone) })
    }

    private static func queueRepo(_ repo: CiPage.QueueRepo, phone: Bool, header: Bool) -> CardJSON {
        let copy = CiPage.Copy.Queue.self
        let ids = CiPage.TestIds.self
        let id = "\(ids.queueRepo).\(repo.repositoryId)"
        var children: [CardJSON] = []
        if header {
            children.append(hstack("\(id).head", [
                node("\(id).title", "Heading", props: ["text": .string(repo.title), "level": .string("h3")]),
                node("\(id).caption", "Text", props: ["text": .string(repo.caption), "variant": .string("muted")]),
            ]))
        }
        if !phone {
            func stat(_ key: String, _ value: String) -> CardJSON {
                node("\(id).stat.\(key)", "StatTile", props: ["label": .string(copy.stats[key] ?? key), "value": .string(value)])
            }
            children.append(node("\(ids.queueStats).\(repo.repositoryId)", "Grid", props: [
                "columns": .object(["base": .number(2), "md": .number(4)]), "gap": .string("sm"),
            ], children: [
                stat("inQueue", String(repo.stats.inQueue)),
                stat("landedToday", String(repo.stats.landedToday)),
                stat("sentBackToday", String(repo.stats.sentBackToday)),
                stat("typicalRun", repo.stats.typicalRun ?? "—"),
            ]))
        }
        for now in repo.testing { children.append(nowCard(now, "\(ids.queueNow).\(now.trainId)")) }
        let upNext = queueSection("\(id).next", copy.upNext, repo.upNext, phone: phone)
        let waiting = queueSection("\(id).waiting", copy.notReady, repo.waiting, phone: phone, fold: true)
        let sentBack = queueSection("\(id).sent", copy.sentBack, repo.sentBack, phone: phone)
        let landed = queueSection("\(id).landed", copy.landed, repo.landed, phone: phone)
        if phone {
            children.append(contentsOf: [upNext, sentBack, waiting, landed].compactMap { $0 })
        } else {
            let left = [upNext, waiting].compactMap { $0 }
            let right = [sentBack, landed].compactMap { $0 }
            if !left.isEmpty, !right.isEmpty {
                children.append(node("\(id).columns", "Grid", props: [
                    "columns": .object(["base": .number(1), "lg": .number(2)]), "gap": .string("lg"),
                ], children: [vstack("\(id).left", left, gap: "lg"), vstack("\(id).right", right, gap: "lg")]))
            } else {
                children.append(contentsOf: left + right)
            }
        }
        return vstack(id, children, gap: "lg")
    }

    private static func queuePanel(_ input: CiPage.Input, phone: Bool, repositoryId: String?) -> CardJSON {
        let ids = CiPage.TestIds.self
        let repos = CiPage.queue(input).filter { repositoryId == nil || $0.repositoryId == repositoryId }
        if repos.isEmpty {
            return node(ids.queue, "EmptyState", props: [
                "icon": .string("pr-batch"), "title": .string(CiPage.Copy.Empty.train), "size": .string("list"),
            ])
        }
        return vstack(ids.queue, repos.map { queueRepo($0, phone: phone, header: repositoryId == nil) }, gap: "xl")
    }

    // MARK: - Runners

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
                props["meta"] = call("fill", ["template": .string(copy.offlineSince), "params": .object(["time": relative(since)])])
            }
            let slots: [String: CardJSON]? = row.queue.map {
                ["trailing": node("\(ids.runnerRow).\(row.id).queue", "Badge", props: ["text": .string($0), "variant": .string("outline")])]
            }
            return node("\(ids.runnerRow).\(row.id)", "Row", props: props, slots: slots, on: ("press", Events.openRunner, ["device": .string(row.id)]))
        })
    }

    // MARK: - The page

    /// The CI page's body (ci-page.json LAYOUT): the CURRENT tab's panel.
    /// The tab switcher is the platform's own segmented control (labels
    /// copy.tabs, phones copy.phoneTabs; counts from `CiPage.tabCounts`),
    /// drawn by the host above this tree.
    public static func ciPageTree(_ input: CiPage.Input, state: State) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        switch state.tab {
        case .train:
            return queuePanel(input, phone: state.phone, repositoryId: nil)
        case .prs:
            return vstack("\(ids.page).prs", [
                filters(input, state),
                subjectSections(
                    ids.prs, CiPage.prGroups(input, filter: state.filter), phone: state.phone,
                    empty: filtered(state.filter) ? copy.Empty.filtered : copy.Empty.prs
                ),
            ])
        case .releases:
            return vstack("\(ids.page).releases", [
                filters(input, state),
                runSections(
                    ids.releases, input, filter: state.filter, kinds: [.release], phone: state.phone,
                    empty: filtered(state.filter) ? copy.Empty.filtered : copy.Empty.releases
                ),
            ])
        case .runners:
            return runnersPanel(input)
        }
    }

    /// THE REPO PAGE'S RUNS TAB (#EXP-2214): a `Segmented` of Merge queue ·
    /// Pull requests · Releases (copy.tabs, phones copy.phoneTabs; `ci-tab`
    /// carries the value), then that panel for ONE repository: no
    /// repository header, no filter bar.
    public static func ciRepoRunsTree(_ input: CiPage.Input, repositoryId: String, tab: CiPage.Tab, phone: Bool) -> CardJSON {
        let ids = CiPage.TestIds.self
        let copy = CiPage.Copy.self
        let labels = phone ? copy.phoneTabs : copy.tabs
        let filter = CiPage.RunFilter(status: .all, repositoryId: repositoryId, source: nil)
        let panel: CardJSON = switch tab {
        case .train: queuePanel(input, phone: phone, repositoryId: repositoryId)
        case .prs: subjectSections(ids.prs, CiPage.prGroups(input, filter: filter), phone: phone, empty: copy.Empty.prs)
        default: runSections(ids.releases, input, filter: filter, kinds: [.release], phone: phone, empty: copy.Empty.releases)
        }
        return vstack(ids.runs, [
            node(ids.tabs, "Segmented", props: [
                "value": .string(tab.rawValue),
                "size": .string("sm"),
                "items": .array(repoTabs.map { option(labels[$0.rawValue] ?? $0.rawValue, $0.rawValue) }),
            ], on: ("change", Events.tab, nil)),
            panel,
        ])
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
        default:
            return nil
        }
        return next
    }
}
