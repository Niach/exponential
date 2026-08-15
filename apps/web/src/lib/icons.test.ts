// EXP-273 — drift gates for the shared icon registry (@exp/icons).
//
// The registry is the single source of icon truth for four clients whose
// generated outputs are COMMITTED, so this file is what stands between a
// stale `icons.json` edit and four silently-diverging apps. Sibling of
// domain-contract.test.ts, which locks the enum values the same way.

import { describe, expect, it } from "vitest"
import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { join } from "node:path"
import { contract } from "@exp/domain-contract"
import { boardIconValues } from "@exp/db-schema/domain"
import registry from "@exp/icons/icons.json" with { type: "json" }
import {
  CUSTOM_ICONS,
  ICON_NAMES,
  PICKABLE_ICONS,
  SEMANTIC_ICONS,
  isIconName,
  isPickableIcon,
} from "@exp/icons"
import { ICON_COMPONENTS, conceptIcon } from "./icons.generated"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const iconsPkg = join(repoRoot, `packages/icons`)

describe(`icon registry`, () => {
  it(`pickable matches the domain contract's boardIcon values`, () => {
    // boards.icon is validated against the contract enum while the picker is
    // rendered from the registry: drift means a user can pick an icon the
    // server rejects, or the server accepts one no client can draw.
    expect(registry.pickable).toEqual(contract.boardIcon.values)
    expect(registry.pickable).toEqual([...boardIconValues])
  })

  it(`pickable is append-only against the original 16`, () => {
    // Reordering or dropping a name orphans every board row that stored it.
    const original = [
      `code`,
      `square-kanban`,
      `megaphone`,
      `bug`,
      `rocket`,
      `book-open`,
      `globe`,
      `heart`,
      `star`,
      `zap`,
      `wrench`,
      `shield`,
      `package`,
      `terminal`,
      `lightbulb`,
      `message-circle`,
    ]
    expect(registry.pickable.slice(0, original.length)).toEqual(original)
  })

  it(`has no duplicate pickable entries`, () => {
    expect(new Set(registry.pickable).size).toBe(registry.pickable.length)
  })

  it(`generated.ts mirrors icons.json`, () => {
    expect([...PICKABLE_ICONS]).toEqual(registry.pickable)
    expect(SEMANTIC_ICONS).toEqual(registry.semantic)
    expect([...CUSTOM_ICONS]).toEqual(Object.keys(registry.custom).sort())
    expect([...ICON_NAMES]).toEqual(
      [
        ...new Set([
          ...registry.pickable,
          ...Object.values(registry.semantic),
          ...Object.keys(registry.custom),
        ]),
      ].sort()
    )
  })

  it(`every lucide registry name has a real, non-deprecated lucide icon`, () => {
    // A typo would otherwise surface as a missing asset on three clients and
    // a broken import on the fourth; an alias ships geometry-less art.
    // Custom (hand-authored) names deliberately have no lucide file.
    const custom = new Set<string>(CUSTOM_ICONS)
    for (const name of ICON_NAMES.filter((n) => !custom.has(n))) {
      const file = join(
        repoRoot,
        `node_modules/lucide-react/dist/esm/icons`,
        `${name}.js`
      )
      expect(existsSync(file), `${name}: no such lucide icon`).toBe(true)
      expect(
        readFileSync(file, `utf8`).includes(`__iconNode`),
        `${name}: deprecated lucide alias — use the canonical name`
      ).toBe(true)
    }
  })

  it(`custom glyphs are single-color and never shadow a lucide name`, () => {
    // EXP-314 pie clocks: the wedge must be a filled, stroke-less path (the
    // SVG wrapper's stroke-width 2 would otherwise fatten it), and the whole
    // glyph must stay one-color — desktop gpui rasterizes SVGs to a single
    // tinted alpha mask.
    const custom = registry.custom as unknown as Record<
      string,
      [string, Record<string, string>][]
    >
    for (const [name, nodes] of Object.entries(custom)) {
      expect(
        existsSync(
          join(repoRoot, `node_modules/lucide-react/dist/esm/icons/${name}.js`)
        ),
        `${name}: shadows a real lucide icon`
      ).toBe(false)
      for (const [tag, attrs] of nodes) {
        if (tag === `path` && `fill` in attrs) {
          expect(attrs.fill).toBe(`currentColor`)
          expect(
            (attrs as Record<string, string>).stroke,
            `${name}: filled wedge must carry stroke="none"`
          ).toBe(`none`)
        }
      }
    }
  })

  it(`every registry name resolves to a web component and a concept`, () => {
    for (const name of ICON_NAMES) {
      expect(ICON_COMPONENTS[name], `${name}: missing component`).toBeTruthy()
    }
    for (const key of Object.keys(SEMANTIC_ICONS)) {
      expect(conceptIcon(key as keyof typeof SEMANTIC_ICONS)).toBeTruthy()
    }
  })

  it(`ships one SVG asset per name to desktop and iOS`, () => {
    for (const name of ICON_NAMES) {
      expect(
        existsSync(join(repoRoot, `apps/desktop/assets/icons/${name}.svg`)),
        `${name}: missing desktop asset`
      ).toBe(true)
      const imageset = join(
        repoRoot,
        `apps/ios/Exponential/Assets.xcassets/lucide-${name}.imageset`
      )
      expect(existsSync(imageset), `${name}: missing iOS imageset`).toBe(true)
      expect(
        existsSync(join(imageset, `lucide-${name}.svg`)) &&
          existsSync(join(imageset, `Contents.json`)),
        `${name}: incomplete iOS imageset`
      ).toBe(true)
    }
  })

  it(`the actionInputType contract carries the icon picker type`, () => {
    expect(contract.actionInputType.values).toContain(`icon`)
  })

  it(`covers every enum value that clients render an icon for`, () => {
    // The inbox derives its icons from `notification-<kebab>` concepts, and
    // the status/priority tables from `status-*`/`priority-*`. A new enum
    // value with no concept would render blank on one client and fall back on
    // another — which is the exact drift this registry exists to remove.
    const kebab = (v: string) => v.replace(/_/g, `-`)
    for (const type of contract.notificationType.values) {
      expect(
        SEMANTIC_ICONS,
        `notification type "${type}" has no registry concept`
      ).toHaveProperty(`notification-${kebab(type)}`)
    }
    for (const status of contract.issueStatus.values) {
      expect(SEMANTIC_ICONS).toHaveProperty(`status-${kebab(status)}`)
    }
    for (const priority of contract.issuePriority.values) {
      expect(SEMANTIC_ICONS).toHaveProperty(`priority-${kebab(priority)}`)
    }
    for (const state of contract.prState.values) {
      expect(SEMANTIC_ICONS).toHaveProperty(`pr-${kebab(state)}`)
    }
  })

  it(`gives each notification type a distinct glyph`, () => {
    // The web inbox used to draw `issue_mention` and `issue_comment` with the
    // same speech bubble, so a mention was indistinguishable from a comment.
    const glyphs = contract.notificationType.values.map(
      (t) =>
        SEMANTIC_ICONS[
          `notification-${t.replace(/_/g, `-`)}` as keyof typeof SEMANTIC_ICONS
        ]
    )
    expect(new Set(glyphs).size).toBe(glyphs.length)
  })

  it(`narrowing guards agree with the generated lists`, () => {
    expect(isIconName(`bug`)).toBe(true)
    expect(isIconName(`definitely-not-an-icon`)).toBe(false)
    expect(isPickableIcon(`code`)).toBe(true)
    // A concept-only glyph is a real icon but must not appear in the picker.
    expect(isPickableIcon(`chevron-right`)).toBe(false)
  })

  it(`committed generated output is current`, () => {
    // Re-run the generator and assert nothing moved. Deliberately compares
    // file contents rather than `git status`, so it behaves the same on a
    // clean checkout, a dirty tree, and in CI.
    const targets = [
      join(iconsPkg, `src/generated.ts`),
      join(repoRoot, `apps/web/src/lib/icons.generated.ts`),
      join(repoRoot, `apps/desktop/crates/ui/src/icons.generated.rs`),
      join(repoRoot, `apps/ios/ExpUI/Sources/AppIcons.generated.swift`),
      join(
        repoRoot,
        `apps/android/app/src/main/java/com/exponential/app/ui/icons/ExpIcons.generated.kt`
      ),
      ...ICON_NAMES.map((n) =>
        join(repoRoot, `apps/desktop/assets/icons/${n}.svg`)
      ),
    ]
    const before = new Map(targets.map((f) => [f, readFileSync(f, `utf8`)]))
    execFileSync(`bun`, [`run`, `scripts/generate.ts`], {
      cwd: iconsPkg,
      stdio: `pipe`,
    })
    for (const [file, contents] of before) {
      expect(
        readFileSync(file, `utf8`),
        `${file} is stale — run \`bun run --filter @exp/icons generate\``
      ).toBe(contents)
    }
  })

  it(`leaves the hand-maintained desktop brand marks alone`, () => {
    // The generator shares apps/desktop/assets/icons with brand art it must
    // never own (the icon_named! macro turns every file there into a variant).
    const brand = [`claude`, `codex`, `pi`, `logo`, `apple`, `google`]
    for (const name of brand) {
      expect(
        existsSync(join(repoRoot, `apps/desktop/assets/icons/${name}.svg`)),
        `${name}.svg went missing — the generator must only write registry names`
      ).toBe(true)
      expect((ICON_NAMES as readonly string[]).includes(name)).toBe(false)
    }
  })

  it(`every desktop SVG is a well-formed 24x24 lucide glyph`, () => {
    // gpui renders these through resvg; a malformed file fails at paint time,
    // not at build time, so check the shape of what we emit.
    const dir = join(repoRoot, `apps/desktop/assets/icons`)
    for (const name of ICON_NAMES) {
      const svg = readFileSync(join(dir, `${name}.svg`), `utf8`)
      expect(svg.startsWith(`<svg `), `${name}: bad header`).toBe(true)
      expect(svg).toContain(`viewBox="0 0 24 24"`)
      expect(svg).toContain(`stroke="currentColor"`)
      expect(svg.trimEnd().endsWith(`</svg>`), `${name}: truncated`).toBe(true)
    }
    // Sanity: the directory holds the registry plus brand marks, nothing odd.
    const files = readdirSync(dir).filter((f) => f.endsWith(`.svg`))
    expect(files.length).toBeGreaterThanOrEqual(ICON_NAMES.length)
    for (const f of files) {
      expect(statSync(join(dir, f)).size).toBeGreaterThan(0)
    }
  })
})

// ---------------------------------------------------------------------------
// EXP-317 — cross-client call-site gates.
//
// The registry only guarantees parity for call sites that USE it. These three
// checks are what keep web and desktop from quietly drawing something else:
// the settings nav (the drift this issue was filed for), gpui-component's own
// icon set on desktop, and lucide's deprecated aliases on web.
// ---------------------------------------------------------------------------

function walk(dir: string, ext: string): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walk(path, ext))
    else if (entry.name.endsWith(ext)) out.push(path)
  }
  return out
}

describe(`icon call sites`, () => {
  it(`web and desktop draw the same settings nav`, () => {
    // The reported EXP-317 symptom: two hand-picked icon lists for one screen.
    // Both sides now name `settings-*` concepts, so this compares the concept
    // each section resolves to rather than the glyph it happens to import.
    const web = readFileSync(
      join(repoRoot, `apps/web/src/routes/t/$teamSlug/settings/-shared.tsx`),
      `utf8`
    )
    const desktop = readFileSync(
      join(repoRoot, `apps/desktop/crates/ui/src/settings/mod.rs`),
      `utf8`
    )

    // web: `label: \`General\`, to: …, icon: conceptIcon(\`settings-general\`)`
    const webNav = new Map<string, string>()
    for (const m of web.matchAll(
      /label: `([^`]+)`,\s*\n\s*to: `[^`]+`,\s*\n\s*icon: conceptIcon\(`([a-z0-9-]+)`\)/g
    )) {
      webNav.set(m[1], m[2])
    }
    // desktop: `SettingsSection::General => Icon::from(registry::SETTINGS_GENERAL)`
    const desktopNav = new Map<string, string>()
    for (const m of desktop.matchAll(
      /SettingsSection::(\w+)(?:\(_\))? => Icon::from\(registry::([A-Z0-9_]+)\)/g
    )) {
      desktopNav.set(m[1], m[2].toLowerCase().replace(/_/g, `-`))
    }

    // Every icon on both screens comes from the registry — no stragglers.
    expect(web).not.toMatch(/icon: [A-Z]\w+,/)
    // EXP-238: + the Personal group (Account, Notifications, API keys).
    expect(webNav.size).toBe(12)
    // EXP-262: + About (desktop-only in the settings nav, like Tools/Agents).
    // EXP-500: + Archived boards, also desktop-only in the NAV — web keeps the
    // same list as a card on its single Boards settings page, which desktop
    // flattened into per-board panes (EXP-288), so it needs its own entry.
    expect(desktopNav.size).toBe(15)

    // The sections both clients render, web label → desktop variant.
    const shared: [string, string][] = [
      [`General`, `General`],
      [`Members`, `Members`],
      [`Labels`, `Labels`],
      [`Statuses`, `Statuses`],
      [`Storage`, `Storage`],
      [`Boards`, `Board`],
      [`Repositories`, `Repositories`],
      [`Account`, `Account`],
      [`Notifications`, `Notifications`],
      [`API keys`, `ApiKeys`],
    ]
    for (const [webLabel, desktopSection] of shared) {
      const concept = webNav.get(webLabel)
      expect(concept, `web settings nav lost "${webLabel}"`).toBeTruthy()
      expect(
        desktopNav.get(desktopSection),
        `settings "${webLabel}" drifted: web ${concept}, desktop ${desktopNav.get(desktopSection)}`
      ).toBe(concept)
      expect(SEMANTIC_ICONS).toHaveProperty(concept!)
    }
  })

  it(`the desktop draws no gpui-component icons outside the window controls`, () => {
    // gpui-component ships its OWN lucide-ish set: its `github` is a filled
    // octocat, its `delete` is a backspace key, and a dozen more glyphs are a
    // different vintage than the registry's. Only the title bar's
    // window-management buttons (no lucide equivalent) may use it.
    const src = join(repoRoot, `apps/desktop/crates/ui/src`)
    const offenders = walk(src, `.rs`)
      .filter((f) => !f.endsWith(`title_bar.rs`))
      .filter((f) => /\bIconName::/.test(readFileSync(f, `utf8`)))
      .map((f) => f.slice(repoRoot.length + 1))
    expect(
      offenders,
      `use crate::icons::registry (or ExpIcon) instead of gpui-component's IconName`
    ).toEqual([])
  })

  it(`web imports only canonical, non-deprecated lucide names`, () => {
    // lucide keeps deprecated aliases importable (`MoreHorizontal`,
    // `Loader2`, `AlertTriangle`, …). They still render, so nothing breaks —
    // they just make the same glyph look like two different icons across the
    // codebase, and some (`Code2` → `code-xml`) are not even the icon the
    // name suggests.
    const iconsDir = join(repoRoot, `node_modules/lucide-react/dist/esm/icons`)
    const kebab = (name: string) =>
      name
        .replace(/(?<=[a-z])(?=[A-Z])/g, `-`)
        .replace(/(?<=[A-Za-z])(?=[0-9])/g, `-`)
        .toLowerCase()
    const bad: string[] = []
    for (const file of walk(join(repoRoot, `apps/web/src`), `.tsx`).concat(
      walk(join(repoRoot, `apps/web/src`), `.ts`)
    )) {
      const source = readFileSync(file, `utf8`)
      for (const block of source.matchAll(
        /import\s*\{([^}]*)\}\s*from\s*"lucide-react"/g
      )) {
        for (const raw of block[1].split(`,`)) {
          const name = raw.trim().split(` as `)[0].replace(/^type /, ``).trim()
          if (!name || name === `LucideIcon` || name === `createLucideIcon`) {
            continue
          }
          // shadcn's generated components import the `<Name>Icon` aliases.
          const candidates = [kebab(name)]
          if (name.endsWith(`Icon`)) candidates.push(kebab(name.slice(0, -4)))
          const resolved = candidates
            .map((c) => join(iconsDir, `${c}.js`))
            .find((p) => existsSync(p))
          if (!resolved) {
            bad.push(`${name} (no such lucide icon)`)
          } else if (!readFileSync(resolved, `utf8`).includes(`__iconNode`)) {
            bad.push(`${name} (deprecated alias)`)
          }
        }
      }
    }
    expect([...new Set(bad)].sort()).toEqual([])
  })
})
