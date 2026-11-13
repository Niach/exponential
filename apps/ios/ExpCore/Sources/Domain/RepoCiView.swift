import Foundation

/// #EXP-1617 E4 part 4 (fixtures/repo-ci.json): the repo page's Runs tab and
/// the CI section of its Settings tab, PHONE layout, as the genUI trees web
/// builds (`apps/web/src/lib/repo-ci-view.ts`: `repoRunsTree`,
/// `repoCiSectionTree`; Android `RepoCiSection.kt`): the same node ids, props
/// and order, painted by `ExponentialSurface` like the run page. The two
/// overlays stay E3's: Delete's AlertDialog = `GlassAlert`, Add secret's
/// phone Drawer = THE sheet, so Add secret is a plain Button firing
/// `Events.addSecret`.
public enum RepoCiView {
    /// Events the trees fire (web `REPO_CI_EVENTS`).
    public enum Events {
        public static let openRun = "repo-run-open"
        public static let required = "repo-ci-required"
        public static let githubStatuses = "repo-ci-github-statuses"
        public static let actionsRunners = "repo-ci-actions-runners"
        public static let deleteSecret = "repo-ci-delete-secret"
        public static let addSecret = "repo-ci-add-secret"

        public static func of(_ key: RepoCi.Switch) -> String {
            switch key {
            case .githubStatuses: githubStatuses
            case .actionsRunners: actionsRunners
            }
        }
    }

    /// The pointer the ChipInput's `values` is bound to (the host writes the
    /// stored list back, so a dropped entry leaves the field).
    public static let requiredPath = "/required"
    /// At most this many required checks (`ci.setRepoConfig`'s cap).
    public static let maxRequired = 64

    private static func iconTone(_ tone: PrChecks.Tone) -> String {
        switch tone {
        case .muted: "neutral"
        case .green: "success"
        case .red: "danger"
        }
    }

    /// repo-page.json `copy.checks[state].tone` → the Chip's dot token (web `DOT`).
    private static func dot(_ state: String) -> String {
        switch RepoPage.Copy.checks(state)?.tone {
        case .green: "$color.success"
        case .amber: "$color.warning"
        case .red: "$color.destructive"
        case nil: "$color.mutedForeground"
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

    private static func stack(_ id: String, _ children: [CardJSON]) -> CardJSON {
        node(id, "Stack", props: ["direction": .string("vertical"), "gap": .string("none"), "align": .string("stretch")], children: children)
    }

    /// PHONES: one `Row` per run (leading glyph tinted, `#n`, the name,
    /// `ref · sha7 · trigger`, the duration); a press opens the run page.
    public static func runsTree(_ rows: [RepoCi.RunRow]) -> CardJSON {
        let ids = RepoCi.TestIds.self
        return node(ids.runs, "Section", children: rows.map { row in
            var props: [String: CardJSON] = [
                "identifier": .string(row.identifier),
                "title": .string(row.title),
                "subtitle": .string(row.subtitle),
                "pressable": .bool(true),
            ]
            if let duration = row.duration { props["meta"] = .string(duration) }
            var extra = on("press", Events.openRun, context: ["run": .string(row.id)])
            extra["slots"] = .object(["leading": node("\(ids.runRow).\(row.id).glyph", "Icon", props: [
                "name": .string(row.glyph.rawValue), "tone": .string(iconTone(row.tone)), "size": .string("sm"),
            ])])
            return node("\(ids.runRow).\(row.id)", "Row", props: props, extra: extra)
        })
    }

    /// The CI section, PHONE layout (web `repoCiSectionTree`): a `Section` CI
    /// → the pipelines (`Row` + trailing `Chip{dot}`; none = muted Text) →
    /// `SettingsRow` Required checks holding the `ChipInput` → `Heading` h4
    /// Secrets, the secret `Row`s (meta = relative time, trailing a ghost sm
    /// `Button` Delete for managers; none = muted Text), the outline sm
    /// `Button` Add secret (managers) → per `switches(provider)` a
    /// `SettingsRow` + `Switch`. `secrets` nil = still loading (that block
    /// waits). `manager` false = every control disabled, no secret actions.
    public static func sectionTree(
        provider: String?,
        pipelines: [RepoCi.PipelineRow],
        suggestions: [String],
        secrets: [RepoCi.Secret]?,
        githubStatuses: Bool,
        actionsRunners: Bool,
        manager: Bool
    ) -> CardJSON {
        let ids = RepoCi.TestIds.self
        let disabled = CardJSON.bool(!manager)
        var children: [CardJSON] = []
        if pipelines.isEmpty {
            children.append(node(ids.pipelines, "Text", props: ["text": .string(RepoCi.Copy.Ci.noPipeline), "variant": .string("muted")]))
        } else {
            children.append(stack(ids.pipelines, pipelines.map { p in
                node("\(ids.pipelines).\(p.runId)", "Row", props: ["title": .string(p.name)], extra: [
                    "slots": .object(["trailing": node("\(ids.pipelines).\(p.runId).state", "Chip", props: [
                        "label": .string(RepoPage.Copy.checks(p.state)?.label ?? p.state),
                        "dot": .string(dot(p.state)),
                    ])]),
                ])
            }))
        }
        children.append(node(
            "repo-ci-required-row", "SettingsRow",
            props: [
                "label": .string(RepoCi.Copy.Ci.requiredLabel),
                "description": .string(RepoCi.Copy.Ci.requiredDescription),
                "stackBelow": .string("md"),
            ],
            children: [
                node(ids.required, "ChipInput", props: [
                    "name": .string(ids.required),
                    "values": .object(["@path": .string(requiredPath)]),
                    "placeholder": .string(RepoCi.Copy.Ci.requiredPlaceholder),
                    "suggestions": .array(suggestions.map { .object(["label": .string($0), "value": .string($0)]) }),
                    "disabled": disabled,
                ], extra: on("change", Events.required)),
            ]
        ))
        if let secrets {
            children.append(node("repo-ci-secrets-heading", "Heading", props: [
                "text": .string(RepoCi.Copy.Secrets.title), "level": .string("h4"),
            ]))
            if secrets.isEmpty {
                children.append(node(ids.secrets, "Text", props: ["text": .string(RepoCi.Copy.Secrets.empty), "variant": .string("muted")]))
            } else {
                children.append(stack(ids.secrets, secrets.map { s in
                    var extra: [String: CardJSON] = [:]
                    if manager {
                        extra["slots"] = .object(["trailing": node(
                            "\(ids.secrets).\(s.name).delete", "Button",
                            props: ["label": .string(RepoCi.Copy.Secrets.delete), "variant": .string("ghost"), "size": .string("sm")],
                            extra: on("press", Events.deleteSecret, context: ["name": .string(s.name)])
                        )])
                    }
                    return node("\(ids.secrets).\(s.name)", "Row", props: [
                        "title": .string(s.name),
                        "meta": .object(["@call": .string("formatRelativeTime"), "args": .object(["value": .string(s.updatedAt)])]),
                    ], extra: extra)
                }))
            }
            if manager {
                children.append(node(
                    ids.addSecret, "Button",
                    props: ["label": .string(RepoCi.Copy.Secrets.add), "variant": .string("outline"), "size": .string("sm")],
                    extra: on("press", Events.addSecret)
                ))
            }
        }
        for key in RepoCi.switches(provider: provider) {
            let copy = RepoCi.Copy.switch(key)
            let checked = key == .githubStatuses ? githubStatuses : actionsRunners
            children.append(node(
                "\(key.testId)-row", "SettingsRow",
                props: ["label": .string(copy.label), "description": .string(copy.description)],
                children: [
                    node(key.testId, "Switch", props: [
                        "label": .string(""), "name": .string(key.testId), "checked": .bool(checked), "disabled": disabled,
                    ], extra: on("change", Events.of(key))),
                ]
            ))
        }
        return node(ids.ci, "Section", props: ["title": .string(RepoCi.Copy.Ci.section)], children: children)
    }

    /// A ChipInput's entries as stored: trimmed, empty / over-255 dropped,
    /// duplicates dropped, at most `maxRequired` (web `repoCiConfigAfter`).
    public static func requiredFrom(_ entries: [String]) -> [String] {
        var out: [String] = []
        for raw in entries {
            let name = raw.trimmingCharacters(in: .whitespaces)
            if !name.isEmpty, name.count <= 255, !out.contains(name) { out.append(name) }
        }
        return Array(out.prefix(maxRequired))
    }
}
