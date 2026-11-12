#!/usr/bin/env bun
// VAPP-213: the three BUILT-IN skills (packages/domain-contract/fixtures/
// skills.json `builtin`), generated from the docs so they never drift from
// them. Run from the repo root with `bun packages/exponential-ui/scripts/
// generate-skills.ts`; apps/web's builtin-skills.test.ts re-renders in
// memory and fails on any committed drift (a doc edit without a regen).
//
// Outputs (all committed, under packages/exponential-ui/skills/):
//   <name>/SKILL.md          frontmatter (name, description) + a compact body
//   <name>/references/*.md   the doc sections, verbatim (images dropped)
//   <name>/examples/*        the worked examples (samples/apps, the Motion gallery)
//   index.json               the manifest: genUI version + per skill name, description, files
//   index.ts                 the same with every file inlined (?raw), for the server
//
// Sources: packages/exponential-ui/docs/package-format.md, docs/spec/apps.md,
// docs/spec/motion.md, samples/apps/*.app.json, the genUI prompt (src/prompt.ts:
// the rules, functions and style sections every model gets), the built-in
// themes, the Motion guide (eval/motion/guide.md + lottie-guide.md, the
// text models get) and the Motion gallery (the 8 recorded replies the guide
// page shows, apps/site/src/components/eui/trees.ts). Components are
// NOT inlined: the skills point to exponential_apps {op: "catalog"}.

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, relative, resolve } from "node:path"
import { editorWorkflow } from "../../domain-contract/src/apps"
import skillsFixture from "../../domain-contract/fixtures/skills.json" with { type: "json" }
import { coreCatalog, isOffered } from "../src/catalog"
import { catalogPrompt, PROMPT_RULES } from "../src/prompt"
import type { ComponentDef } from "../src/types"
import pkg from "../package.json" with { type: "json" }

const EUI = resolve(import.meta.dirname, `..`)
const ROOT = resolve(EUI, `../..`)
export const SKILLS_DIR = join(EUI, `skills`)

/** The Motion gallery, in the guide page's order (the drift test checks the page imports exactly these). */
export const MOTION_GALLERY = [`pulse-dot`, `progress-ring`, `typing-dots`, `heartbeat`, `equalizer`, `error-x`, `sun`, `wifi`] as const

const SAMPLE_APPS = [`crm`, `leave-planner`, `standup`, `time-tracker`] as const

const DESCRIPTIONS: Record<string, string> = {
  "exponential-apps": `Build or change an Exponential app (Apps): the package format (formatVersion 5), record kinds and their policies, named queries, CEL expressions and functions, optional values, actions with require, package tests, exposes, draft vs published, with three complete sample apps as worked examples. Use when writing or editing an app package (exponential_apps_write upsert) or its records.`,
  "exponential-genui": `Write genUI (A2UI v0.9) surfaces: the component model, bindings and calls, actions, layout and responsive rules, the client functions, style keys, tokens and the built-in themes. Use when composing an exponential_show card, an app template or any genUI surface; exponential_apps {op: "catalog"} lists the components and their props.`,
  "exponential-motion": `Animate with Motion: the readable Motion dialect that compiles 1:1 to Lottie, Lottie and dotLottie (.lottie) sources, playback, dotLottie state machines with inputs and events, the exponential motion import/compile/decompile/check/render CLI, and eight worked examples. Use when adding a Motion component to a surface or an app, or converting a Lottie file.`,
}

const read = (path: string) => readFileSync(join(ROOT, path), `utf8`)

/** The section that starts at the heading line `heading` (exact text, any
 *  level), up to the next heading of the same or a higher level. */
export function section(md: string, heading: string): string {
  const lines = md.split(`\n`)
  const start = lines.findIndex((line) => /^#+ /.test(line) && line.replace(/^#+ /, ``).trim() === heading)
  if (start < 0) throw new Error(`generate-skills: no heading "${heading}"`)
  const level = /^#+/.exec(lines[start]!)![0].length
  let end = lines.length
  let fenced = false
  for (let i = start + 1; i < lines.length; i++) {
    if (lines[i]!.startsWith("```")) fenced = !fenced
    const m = !fenced && /^(#+) /.exec(lines[i]!)
    if (m && m[1]!.length <= level) {
      end = i
      break
    }
  }
  return tidy(lines.slice(start, end).join(`\n`))
}

/** Drop images (their relative paths do not travel), `---` rules and runs of blank lines. */
function tidy(md: string): string {
  return `${md
    .split(`\n`)
    .filter((line) => !/^!\[[^\]]*\]\([^)]*\)\s*$/.test(line) && line.trim() !== `---`)
    .join(`\n`)
    .replace(/\n{3,}/g, `\n\n`)
    .trim()}\n`
}

/** Demote every heading by `by` levels (a section pasted under a page title). */
function demote(md: string, by: number): string {
  let fenced = false
  return md
    .split(`\n`)
    .map((line) => {
      if (line.startsWith("```")) fenced = !fenced
      return !fenced && /^#+ /.test(line) ? `${`#`.repeat(by)}${line}` : line
    })
    .join(`\n`)
}

function page(title: string, from: string, body: string): string {
  return `# ${title}\n\n> Generated from ${from} (genUI ${pkg.version}) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.\n\n${body.trim()}\n`
}

function skillMd(name: string, body: string): string {
  const description = JSON.stringify(DESCRIPTIONS[name])
  return `---\nname: ${name}\ndescription: ${description}\n---\n\n${body.trim()}\n`
}

type Files = Map<string, string>

// ── exponential-apps ────────────────────────────────────────────────────

function appsSkill(): Files {
  const format = read(`packages/exponential-ui/docs/package-format.md`)
  const spec = read(`docs/spec/apps.md`)
  const formatPath = `packages/exponential-ui/docs/package-format.md`
  const files: Files = new Map()
  const refs: { path: string; title: string }[] = []
  const ref = (path: string, title: string, from: string, body: string) => {
    files.set(path, page(title, from, body))
    refs.push({ path, title })
  }
  ref(
    `references/package.md`,
    `The package and its versions`,
    `${formatPath} (Versions, The package, The package file) and docs/spec/apps.md §2.3`,
    [section(format, `Versions`), section(format, `The package`), section(format, `The package file`), demote(section(spec, `2.3 Versions and channels`), 0)].join(`\n`)
  )
  ref(`references/records.md`, `Records: kinds, policies, queries, writes, RecordForm / RecordTable`, `${formatPath} (Records, Record macros, Public links) and docs/spec/apps.md §3.4`, [section(format, `Records`), section(format, `Record macros`), section(format, `Public links`), section(spec, `3.4 Permissions`)].join(`\n`))
  ref(`references/import.md`, `Importing a spreadsheet (CSV, TSV, xlsx) into records`, `docs/spec/apps.md §10`, section(spec, `10. Import and export`))
  ref(`references/expressions.md`, `Expressions (CEL), functions and optional values`, `${formatPath} (Expressions)`, section(format, `Expressions`))
  ref(`references/actions.md`, `Actions: all-or-nothing writes, require`, `${formatPath} (Actions)`, section(format, `Actions`))
  ref(`references/tests.md`, `Package tests`, `${formatPath} (Tests)`, section(format, `Tests`))
  ref(
    `references/tools-and-exposes.md`,
    `Tools, exposes and extensions`,
    `${formatPath} (Tools, Exposes, Extensions)`,
    [section(format, `Tools`), section(format, `Exposes`), section(format, `Extensions`)].join(`\n`)
  )
  ref(
    `references/automations.md`,
    `Automations: schedule, record, event and webhook triggers; agent hand-off`,
    `docs/spec/apps.md §7.1-§7.3`,
    [section(spec, `7.1 Triggers`), section(spec, `7.2 Webhooks (✅ A3 server, #VAPP-237)`), section(spec, `7.3 Agent hand-off (✅ A2, #VAPP-236)`)].join(`\n`)
  )

  const examples = SAMPLE_APPS.map((id) => {
    // VAPP-234: a sample is a signed app file (the template gallery); the
    // worked example is its package.
    const app = (JSON.parse(read(`samples/apps/${id}.app.json`)) as { package: unknown }).package as {
      name: string
      description?: string
      records?: Record<string, { policy?: Record<string, string> }>
      queries?: Record<string, unknown>
      actions?: Record<string, unknown>
      tests?: unknown[]
      templates: Record<string, unknown>
    }
    const path = `examples/${id}.app.json`
    files.set(path, `${JSON.stringify(app, null, 2)}\n`)
    const kinds = Object.entries(app.records ?? {}).map(([kind, def]) => (def.policy ? `${kind} (policy ${Object.entries(def.policy).map(([k, v]) => `${k} ${v}`).join(`, `)})` : kind))
    const parts = [
      `kinds ${kinds.join(`, `) || `none`}`,
      `${Object.keys(app.queries ?? {}).length} named queries`,
      `${Object.keys(app.actions ?? {}).length} actions`,
      `${(app.tests ?? []).length} tests`,
      `template ${Object.keys(app.templates).join(`, `)}`,
    ]
    return `- \`${path}\` — **${app.name}**: ${app.description ?? ``} (${parts.join(`; `)})`
  })

  const overview = section(spec, `0. In one paragraph`)
    .split(`\n`)
    .slice(1)
    .join(`\n`)
    .trim()
    .split(/\n\n/)[0]!
    .replace(/ Three executors run an app, and none of them runs app code:$/, ``)

  const body = [
    `# Exponential apps`,
    `> Generated from the Apps docs (genUI ${pkg.version}) — do not edit; regenerate with packages/exponential-ui/scripts/generate-skills.ts.`,
    overview,
    `## Workflow`,
    editorWorkflow(`<slug>`),
    `Components and their props are NOT listed here: read exponential_apps {op: "catalog"} once (only: [names] for just a few), and the \`exponential-ui\` skill for the surface rules. Animations: the \`exponential-motion\` skill.`,
    `## Checklist`,
    [
      `- \`formatVersion\` is exactly 5; \`sources\` lists every source a binding follows (\`[]\` for none) and \`functions\` every function a press calls.`,
      `- Declare every record kind with a schema; pick its \`policy\` (read/create/update/delete: member | author | owners; create never author) before writing screens.`,
      `- A kind's add/edit form or its list of records = ONE \`RecordForm\` / \`RecordTable\` node (\`references/records.md\`, Record macros), never a hand-built Form or Table.`,
      `- Prefer a named query (group, aggregate, running) over summing rows in expressions.`,
      `- A field not in \`required\` may be missing: guard it (\`has(r.f)\`, \`orElse(r.f, d)\`), else the upsert is refused at that path. Every \`{call: "expr"}\` has a \`default\`.`,
      `- Writes that must land together are one action; check inputs with \`require\` steps.`,
      `- Add a test for every expression, query and action: a failing test refuses the upsert as \`/tests/<i>\`.`,
      `- A spreadsheet to import: \`references/import.md\` (map, dry run, ask, then write in batches).`,
      `- Save a draft early and often; publish only when the person asks.`,
    ].join(`\n`),
    `## References (read on demand)`,
    refs.map((r) => `- \`${r.path}\` — ${r.title}.`).join(`\n`),
    `## Worked examples (complete packages)`,
    examples.join(`\n`),
  ].join(`\n\n`)
  files.set(`SKILL.md`, skillMd(`exponential-apps`, body))
  return files
}

// ── exponential-genui ──────────────────────────────────────────────────────

function uiSkill(): Files {
  const files: Files = new Map()
  const groups = new Map<string, string[]>()
  for (const [name, def] of Object.entries(coreCatalog.components as Record<string, ComponentDef>)) {
    if (!isOffered(def)) continue
    const group = (def as { group?: string }).group ?? `other`
    groups.set(group, [...(groups.get(group) ?? []), name])
  }
  const strip = (text: string) => text.split(`\n`).slice(1).join(`\n`).trim()
  files.set(`references/functions.md`, page(`Client functions`, `the genUI prompt (src/prompt.ts, section functions)`, strip(catalogPrompt({ sections: [`functions`] }))))
  files.set(`references/style.md`, page(`Style keys and tokens`, `the genUI prompt (src/prompt.ts, section style)`, strip(catalogPrompt({ sections: [`style`] }))))
  const themes = JSON.parse(read(`packages/exponential-ui/docs/themes.generated.json`)) as { themes: { id: string; name: string; chain: string[] }[] }
  files.set(
    `references/themes.md`,
    page(
      `Built-in themes`,
      `packages/exponential-ui/docs/themes.generated.json`,
      [
        `A package or surface names one built-in theme by id (an app's \`theme\`); colours are \`$color.*\` tokens, so light and dark both apply. Never write #hex in a surface.`,
        themes.themes.map((t) => `- \`${t.id}\` — ${t.name}${t.chain.length > 1 ? ` (extends ${t.chain.slice(1).join(` → `)})` : ``}`).join(`\n`),
      ].join(`\n\n`)
    )
  )
  const body = [
    `# genUI`,
    `> Generated from the genUI prompt and catalog (genUI ${pkg.version}) — do not edit; regenerate with packages/exponential-ui/scripts/generate-skills.ts.`,
    `genUI paints A2UI v0.9 surfaces the same on web, desktop, iOS and Android: an \`exponential_show\` card, an app template and every other surface use this one model.`,
    `## Rules`,
    PROMPT_RULES.map((rule) => `- ${rule}`).join(`\n`),
    `## Components`,
    `Their props are in exponential_apps {op: "catalog"} (only: [names] = just these). By group:`,
    [...groups].map(([group, names]) => `- ${group}: ${names.join(`, `)}`).join(`\n`),
    `## References (read on demand)`,
    [
      `- \`references/functions.md\` — the client functions a binding or press calls ({call, args}): validation, formatting, logic.`,
      `- \`references/style.md\` — the Box style keys, conditions and the token groups.`,
      `- \`references/themes.md\` — the built-in themes.`,
    ].join(`\n`),
    `Apps add their own bindings (exp: and records: sources) and host functions: the \`exponential-apps\` skill.`,
  ].join(`\n\n`)
  files.set(`SKILL.md`, skillMd(`exponential-genui`, body))
  return files
}

// ── exponential-motion ──────────────────────────────────────────────────

function motionSkill(): Files {
  const spec = read(`docs/spec/motion.md`)
  const files: Files = new Map()
  files.set(`references/spec.md`, page(`Motion: the spec`, `docs/spec/motion.md`, demote(tidy(spec.split(`\n`).slice(1).join(`\n`)), 0)))
  files.set(`references/lottie.md`, page(`Writing Lottie directly`, `packages/exponential-ui/eval/motion/lottie-guide.md`, read(`packages/exponential-ui/eval/motion/lottie-guide.md`)))
  const examples = MOTION_GALLERY.map((id) => {
    const fixture = JSON.parse(read(`packages/exponential-ui/eval/motion/fixtures/dialect/${id}.json`)) as { prompt: string; text: string }
    const m = /```json([\s\S]*?)```/.exec(fixture.text)
    const doc = JSON.parse(m ? m[1]! : fixture.text) as unknown
    const path = `examples/${id}.json`
    files.set(path, `${JSON.stringify(doc, null, 2)}\n`)
    return `- \`${path}\` — ${fixture.prompt}`
  })
  const overview = section(spec, `0. In one paragraph`).split(`\n`).slice(1).join(`\n`).split("```")[0]!.trim()
  const body = [
    `# Motion`,
    `> Generated from docs/spec/motion.md and the Motion guide (genUI ${pkg.version}) — do not edit; regenerate with packages/exponential-ui/scripts/generate-skills.ts.`,
    overview,
    `## Sources`,
    section(spec, `1. Sources`).split(`\n`).slice(1).join(`\n`).trim(),
    `## The dialect (what you write)`,
    read(`packages/exponential-ui/eval/motion/guide.md`).trim(),
    `Put the document in a \`Motion\` component's props (the node's props ARE the document), with \`label\` for accessibility. The Motion component's own props (playback, inputs, stateMachine, on): exponential_apps {op: "catalog", only: ["Motion"]}.`,
    `## Tooling`,
    section(spec, `8. Tooling`).split(`\n`).slice(1).join(`\n`).trim(),
    `## References (read on demand)`,
    [
      `- \`references/spec.md\` — the whole spec: limits, the dialect's full grammar, playback, interactivity (state machines, inputs, events), out of scope.`,
      `- \`references/lottie.md\` — writing or editing raw Lottie JSON instead of the dialect.`,
    ].join(`\n`),
    `## Worked examples (dialect documents)`,
    examples.join(`\n`),
  ].join(`\n\n`)
  files.set(`SKILL.md`, skillMd(`exponential-motion`, body))
  return files
}

// ── the set ─────────────────────────────────────────────────────────────

const BUILDERS: Record<string, () => Files> = {
  "exponential-apps": appsSkill,
  "exponential-genui": uiSkill,
  "exponential-motion": motionSkill,
}

/** Every generated file, path relative to skills/ → content. */
export function buildSkills(): Map<string, string> {
  const names = [...skillsFixture.builtin.names].sort()
  const out = new Map<string, string>()
  const manifest = names.map((name) => {
    const build = BUILDERS[name]
    if (!build) throw new Error(`generate-skills: no builder for the built-in "${name}"`)
    const files = build()
    const body = files.get(`SKILL.md`)!
    for (const [path, content] of [...files].sort(([a], [b]) => (a < b ? -1 : 1))) out.set(`${name}/${path}`, content)
    return {
      name,
      description: DESCRIPTIONS[name]!,
      bytes: new TextEncoder().encode(body).length,
      files: [...files.keys()].filter((path) => path !== `SKILL.md`).sort(),
    }
  })
  out.set(
    `index.json`,
    `${JSON.stringify(
      {
        $comment: `AUTO-GENERATED by packages/exponential-ui/scripts/generate-skills.ts — do not edit. The built-in skills (packages/domain-contract/fixtures/skills.json \`builtin\`), versioned with genUI.`,
        version: pkg.version,
        skills: manifest,
      },
      null,
      2
    )}\n`
  )
  const imports: string[] = []
  const entries = manifest.map((skill, i) => {
    imports.push(`import s${i} from "./${skill.name}/SKILL.md?raw"`)
    const files = skill.files.map((path, j) => {
      imports.push(`import s${i}f${j} from "./${skill.name}/${path}?raw"`)
      return `      { path: ${JSON.stringify(path)}, content: s${i}f${j} },`
    })
    return [`  {`, `    name: ${JSON.stringify(skill.name)},`, `    description: ${JSON.stringify(skill.description)},`, `    skillMd: s${i},`, `    files: [`, ...files, `    ],`, `  },`].join(`\n`)
  })
  out.set(
    `index.ts`,
    [
      `// AUTO-GENERATED by packages/exponential-ui/scripts/generate-skills.ts — do not edit.`,
      `// The built-in skills with every file inlined (Vite ?raw), for the server.`,
      ...imports,
      ``,
      `export interface BuiltinSkillFile {`,
      `  path: string`,
      `  content: string`,
      `}`,
      ``,
      `export interface BuiltinSkillSource {`,
      `  name: string`,
      `  description: string`,
      `  /** The whole SKILL.md: frontmatter + body. */`,
      `  skillMd: string`,
      `  files: BuiltinSkillFile[]`,
      `}`,
      ``,
      `export const BUILTIN_SKILLS_VERSION = ${JSON.stringify(pkg.version)}`,
      ``,
      `export const BUILTIN_SKILL_SOURCES: BuiltinSkillSource[] = [`,
      ...entries,
      `]`,
      ``,
    ].join(`\n`)
  )
  return out
}

/** The files on disk under skills/ (relative path → content). */
export function readSkillsDir(dir = SKILLS_DIR): Map<string, string> {
  const out = new Map<string, string>()
  const walk = (at: string) => {
    if (!existsSync(at)) return
    for (const name of readdirSync(at).sort()) {
      const path = join(at, name)
      if (statSync(path).isDirectory()) walk(path)
      else out.set(relative(dir, path), readFileSync(path, `utf8`))
    }
  }
  walk(dir)
  return out
}

if (import.meta.main) {
  const files = buildSkills()
  rmSync(SKILLS_DIR, { recursive: true, force: true })
  for (const [path, content] of files) {
    const target = join(SKILLS_DIR, path)
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, content)
  }
  console.log(`generate-skills: ${files.size} files under ${relative(ROOT, SKILLS_DIR)}`)
}
