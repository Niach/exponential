import { mkdtempSync, readFileSync, readdirSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import type { ReferenceSource } from "../sync-reference.ts"

/* Docs area (Get started, Apps, Build, Reference): pages generated from
   the repo. Markdown sources pass through; for TS and Rust sources the
   `transform` ignores the file text and renders the page from the parsed
   source, so `source` and the edit link still name the file of truth.
   Links to site pages are written as <a href="/…"> because
   sync-reference rewrites every relative markdown link to GitHub. */

const REPO = resolve(import.meta.dir, `../../../..`)
const read = (path: string) => readFileSync(join(REPO, path), `utf8`)
const link = (href: string, text: string) => `<a href="${href}">${text}</a>`

/** Starlight's (github-slugger's) heading id. */
const slug = (heading: string) =>
  heading
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, ``)
    .replace(/\s/g, `-`)

/** `## N. Title` sections of a markdown file, by number. */
function sections(markdown: string, level = 2): { heading: string; body: string }[] {
  const marker = `${`#`.repeat(level)} `
  const parts = markdown.split(new RegExp(`^(?=${marker})`, `m`)).filter((p) => p.startsWith(marker))
  return parts.map((part) => {
    const [first, ...rest] = part.split(`\n`)
    return { heading: first!.slice(marker.length), body: rest.join(`\n`) }
  })
}

/* ── Apps: spec images live at /eapps/ (the old site's URLs) ── */
const SPEC_IMAGES: Record<string, string> = { "m5-admin.webp": `admin-console.webp` }
const specImages = (markdown: string) =>
  markdown.replace(/!\[([^\]]*)\]\(apps\/([^)]+\.webp)\)/g, (_, alt: string, file: string) => {
    return `<img src="/eapps/${SPEC_IMAGES[file] ?? file}" alt="${alt.replace(/"/g, `&quot;`)}" loading="lazy" />`
  })

function eappsLimits(): string {
  const limits = sections(read(`docs/spec/apps.md`)).find((s) => /Performance limits/.test(s.heading))
  if (!limits) throw new Error(`docs/spec/apps.md lost its "Performance limits" section`)
  return [
    `## Limits`,
    ``,
    `From ${link(`/docs/reference/spec/apps/#${slug(limits.heading)}`, `the Apps spec`)} (✅ = enforced today, 🆕 = planned).`,
    limits.body.trimEnd(),
    ``,
  ].join(`\n`)
}

/* ── Spec index ── */
const SPEC_FILES: { source: string; slug: string; title: string; description: string }[] = [
  {
    source: `docs/spec/apps.md`,
    slug: `docs/reference/spec/apps`,
    title: `Apps spec`,
    description: `The approved Apps spec: package, records, queries, UI, actions, automation, MCP, access, security, runtime, sync, limits, the gap list and the decisions.`,
  },
  ...readdirSync(join(REPO, `packages/exponential-ui/docs`))
    .filter((f) => /^round-\d+-contract\.md$/.test(f))
    .sort((a, b) => Number(a.match(/\d+/)![0]) - Number(b.match(/\d+/)![0]))
    .map((file) => {
      const n = file.match(/\d+/)![0]
      return {
        source: `packages/exponential-ui/docs/${file}`,
        slug: `docs/reference/spec/genui-round-${n}`,
        title: `genUI round ${n} contract`,
        description: `The contract of genUI's round ${n}: what every renderer implements, as written for the round.`,
      }
    }),
]

function specIndex(): string {
  const rows = SPEC_FILES.map((s) => `| ${link(`/${s.slug}/`, s.title)} | \`${s.source}\` |`)
  const apps = sections(read(`docs/spec/apps.md`)).filter((s) => /^(1[7-9]|2\d)\./.test(s.heading))
  return [
    `Specs are the repo's design documents. Each page here is generated from its markdown file; edit the file to change it.`,
    ``,
    `| spec | source |`,
    `|---|---|`,
    `| ${link(`/docs/sdk/motion/reference/`, `Motion`)} | \`docs/spec/motion.md\` |`,
    ...rows,
    ``,
    `## Apps: status and decisions`,
    ``,
    `The Apps spec ends with its gap list, the decisions taken on it and the readiness verdict:`,
    ``,
    ...apps.map((s) => `- ${link(`/docs/reference/spec/apps/#${slug(s.heading)}`, s.heading.replace(/^\d+\.\s*/, ``))}`),
    ``,
    `The user-facing parts (the package format and its limits) are on ${link(`/docs/sdk/apps/reference/`, `the Apps package reference`)}.`,
    ``,
  ].join(`\n`)
}

/* ── Changelog ── */
async function changelog(): Promise<string> {
  const { CHANGELOG } = (await import(join(REPO, `apps/web/src/lib/changelog.ts`))) as {
    CHANGELOG: { id: string; date: string; title: string; summary: string; body: string }[]
  }
  const out = [`What's new in Exponential, newest first: the same entries as the app's What's new card. SDK modules keep their own changelogs (genUI: \`packages/exponential-ui/CHANGELOG.md\`).`, ``]
  let month = ``
  for (const entry of CHANGELOG) {
    const m = entry.date.slice(0, 7)
    if (m !== month) {
      month = m
      const label = new Date(`${m}-01T00:00:00Z`).toLocaleDateString(`en-GB`, { month: `long`, year: `numeric`, timeZone: `UTC` })
      out.push(`## ${label}`, ``)
    }
    out.push(`### ${entry.title}`, ``, `<small>${entry.date}</small>`, ``, entry.summary, ``, entry.body.trim(), ``)
  }
  return out.join(`\n`)
}

/* ── CLI: the USAGE strings of the Rust sources ── */
function rustUsage(path: string): string {
  const m = read(path).match(/const USAGE: &str = "\\\n([\s\S]*?)";/)
  if (!m) throw new Error(`${path}: no USAGE string`)
  return m[1]!.replace(/\\"/g, `"`).replace(/\\\\/g, `\\`).trimEnd()
}

const CLI = `apps/desktop/crates/cli/src`

function cliCommands(usage: string): string {
  const block = usage.split(/\n\n/).find((b) => b.startsWith(`Commands:`))
  if (!block) throw new Error(`main.rs USAGE lost its Commands: block`)
  const rows: { cmd: string; desc: string }[] = []
  for (const line of block.split(`\n`).slice(1)) {
    const m = line.match(/^ {2}(\S.*?)(?:\s{2,}(\S.*))?$/)
    if (m) rows.push({ cmd: m[1]!.trim(), desc: m[2] ?? `` })
    else if (rows.length && line.trim()) rows[rows.length - 1]!.desc += line.trim()
  }
  const cell = (s: string) => s.replace(/\|/g, `\\|`)
  return [`| command | does |`, `|---|---|`, ...rows.map((r) => `| \`${cell(r.cmd)}\` | ${cell(r.desc)} |`)].join(`\n`)
}

function cli(): string {
  const main = rustUsage(`${CLI}/main.rs`)
  return `${CLI_GUIDE}
## Commands

${cliCommands(main)}

## Full usage

These blocks are the CLI's own help text (\`exponential help\`, \`exponential sessions help\`, …).

\`\`\`text
${main}
\`\`\`

### sessions

\`\`\`text
${rustUsage(`${CLI}/commands/sessions.rs`)}
\`\`\`

### mcp

\`\`\`text
${rustUsage(`${CLI}/commands/mcp.rs`)}
\`\`\`

### motion

Convert, check and preview ${link(`/docs/sdk/motion/`, `Motion`)} animations (the dialect is in the ${link(`/docs/sdk/motion/reference/`, `Motion reference`)}).

\`\`\`text
${rustUsage(`${CLI}/commands/motion.rs`)}
\`\`\`
`
}

const CLI_GUIDE = `The \`exponential\` CLI signs a machine in, runs sessions from the terminal and turns a server into a device that runs your team's sessions.

## Install

\`\`\`sh
curl -fsSL https://exponential.at/install.sh | sh
# self-hosted
curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://issues.example.com sh
\`\`\`

Linux (x86_64, arm64) and macOS (Apple Silicon). The script checks the binary's checksum, installs it to \`~/.local/bin/exponential\`, turns on auto-update, signs you in, installs the daemon as a service and prints the machine's readiness (the same block as \`exponential doctor\`). The web app's **Add device** dialog gives you the same command with a one-time install token (\`EXP_INSTALL_TOKEN=expi_…\`), which skips the approval step.

| variable | effect |
|---|---|
| \`EXP_INSTANCE\` | your instance's URL |
| \`EXP_INSTALL_TOKEN\` | one-time \`expi_…\` token from **Add device** (15 minutes) |
| \`EXP_TOKEN\` | sign in with an API key (\`expu_…\`, Settings → Security) |
| \`EXP_DEVICE_LABEL\` | the machine's name |
| \`EXP_NO_DAEMON=1\` / \`EXP_NO_AUTOUPDATE=1\` | skip the daemon service / auto-update |
| \`EXP_INSTALL_DIR\` | install somewhere other than \`~/.local/bin\` |

You also need \`git\` and a coding agent: Claude Code on your \`PATH\`, or Codex (the machine downloads its own pinned build).

## Sign in

\`exponential login\` prints a code and a link to \`<instance>/auth/device\`; approve it in any signed-in browser. It works the same over SSH. Signing in does not make the machine a device: only the daemon does (\`exponential status\` tells you which).

## Run as a daemon

\`exponential daemon install\` writes a systemd user unit (Linux, with lingering turned on where allowed, else it prints \`sudo loginctl enable-linger $USER\`) or a launchd agent (macOS). The machine then appears under **Devices**, and sessions started on it from any client run there with the same launcher as the desktop app. Its name, sharing, agent logins, launch defaults and updates are all in **Device settings** on every client. A restarted daemon claims its unfinished sessions back.

## Other devices

\`--device <name|id>\` on \`code\` or \`run\` starts the session on another of your devices (it must be online); \`--follow\` tails it. \`sessions log\` streams a transcript from the device that ran it over P2P, end-to-end encrypted.

\`\`\`sh
exponential devices
exponential code EXP-42 --device "build box" --agent claude --follow
exponential sessions message 1a2b3c4d "Use the staging database instead"
\`\`\`

## Updating

Auto-update is on unless you opted out: commands check once a day, the daemon every 15 minutes and restarts between turns. \`exponential update\` updates now; \`exponential update --auto off\` turns it off. Release notes: ${link(`/docs/reference/changelog/`, `Changelog`)}.
`

/* ── MCP: the live tool definitions ── */
interface McpParam { name: string; type: string; required: boolean; description: string }
interface McpTool { name: string; alwaysLoad: boolean; blurb: string; ops: { op: string; description: string; params: McpParam[] }[]; params?: McpParam[] }

function mcpSurface(): { groups: { id: string; label: string; tools: McpTool[] }[] } {
  const out = join(mkdtempSync(join(tmpdir(), `site-mcp-`)), `tools.json`)
  const run = Bun.spawnSync([`bun`, resolve(import.meta.dir, `../gen-docs-mcp.ts`), out], {
    cwd: join(REPO, `apps/web`),
    // Registering the tools reads env at import time; nothing connects.
    env: { ...process.env, DATABASE_URL: `postgres://docs:docs@127.0.0.1:1/docs`, CLOUD_INSTANCE: `true`, BETTER_AUTH_URL: `https://exponential.at` },
    stdout: `pipe`,
    stderr: `pipe`,
  })
  if (run.exitCode !== 0) throw new Error(`gen-docs-mcp failed:\n${run.stderr.toString()}`)
  return JSON.parse(readFileSync(out, `utf8`))
}

const esc = (s: string) => s.replace(/\|/g, `\\|`).replace(/</g, `&lt;`).replace(/\n+/g, ` `)
const signature = (params: McpParam[]) => params.map((p) => (p.required ? p.name : `${p.name}?`)).join(`, `)

function mcpReference(): string {
  const { groups } = mcpSurface()
  const count = groups.reduce((n, g) => n + g.tools.length, 0)
  const out = [
    `## Tool reference`,
    ``,
    `${count} tools in ${groups.length} groups, one per noun. A call carries an \`op\` and that op's fields beside it: \`exponential_issues {op: "get", id: "EXP-42"}\`. Every caller gets the same tools; an op that needs a session or the cloud answers with a message saying so. Calls stay inside the OAuth grant or the API key's scope. Issue parameters take an identifier (\`EXP-42\`) wherever they take a UUID. Parameters with \`?\` are optional. Tools marked *always loaded* are in an agent's context from its first turn; the rest load through tool search.`,
    ``,
  ]
  for (const g of groups) {
    out.push(`### ${g.label}`, ``)
    for (const t of g.tools) {
      out.push(`#### \`${t.name}\``, ``)
      if (t.alwaysLoad) out.push(`*Always loaded.*`, ``)
      if (t.blurb) out.push(esc(t.blurb), ``)
      if (t.ops.length) {
        for (const o of t.ops) out.push(`- \`${o.op}{${signature(o.params)}}\`: ${esc(o.description).replace(/\\\|/g, `|`)}`)
        out.push(``)
      } else if (t.params) {
        out.push(`Parameters: \`{${signature(t.params)}}\``, ``)
      }
    }
  }
  return out.join(`\n`)
}

const MCP_GUIDE = `Every Exponential instance is an MCP server. Agents in sessions get it wired in by the launcher; any other client connects with the URL below.

## Connect a client

\`\`\`text
https://app.exponential.at/api/mcp
\`\`\`

Self-hosted: \`https://<your-instance>/api/mcp\`. Streamable HTTP only.

**OAuth** (interactive clients): add the URL with no credentials; the client registers itself and opens your browser on a scope picker (everything, some teams, or some boards). Run consent again to change the grant. **API key** (scripts, CI): create one under Settings → Security and send \`Authorization: Bearer expu_…\`. A key without a scope acts as you with your full membership; a scoped key works for MCP only.

| client | setup |
|---|---|
| Claude (desktop, claude.ai) | Settings → Connectors → Add custom connector, paste the URL, Connect. Connectors dial from Anthropic's cloud, so the instance must be reachable from the internet. |
| ChatGPT | Settings → Apps & Connectors → Advanced → Developer mode, then Create connector with OAuth. |
| Claude Code | \`claude mcp add --transport http --scope user exponential https://app.exponential.at/api/mcp\`, then \`/mcp\` to sign in (or add \`--header "Authorization: Bearer expu_…"\`). |
| Codex CLI | \`codex mcp add exponential --url https://app.exponential.at/api/mcp\` and \`codex mcp login exponential\`; or \`bearer_token_env_var\` in \`config.toml\`. |
| Cursor | \`{"mcpServers": {"exponential": {"url": "https://app.exponential.at/api/mcp"}}}\` in \`~/.cursor/mcp.json\`, then Connect. |
| OpenClaw | \`openclaw plugins install clawhub:@exponential/openclaw-plugin\` with \`EXPONENTIAL_API_KEY\`, or \`openclaw mcp add … --auth oauth\`. |
| stdio-only clients | \`npx mcp-remote https://app.exponential.at/api/mcp\` |

Clients that render MCP Apps show \`exponential_issues\` op \`show\`, \`exponential_notifications\` op \`list\` and \`exponential_devices\` op \`list\` as Exponential's own views.

**Interactive in Claude (MCP Apps).** Outside a session \`exponential_show\` draws its surface inline with the genUI renderer; a Button whose press is a \`reply\` event sends its text as your next message. \`exponential_apps\` op \`get\` runs the app in the conversation: its records, bindings, actions and screens work through the connector, as you, and a call the app is not granted asks first. Op \`list\` lists the apps to open.

Session content is end-to-end encrypted: a client connected straight to \`/api/mcp\` sees a session's metadata (who, when, state) and none of its text.

## App tools

A team's ${link(`/docs/sdk/apps/`, `Apps`)} expose their tools at \`/api/mcp/apps\`, each as \`<app>__<tool>\`, running as you. Connecting asks for its own consent: a team, all apps or a pick, then Read only or Full. Owners set each member's access per app and can switch single tools off.

## Your own MCP servers

Owners add the team's other MCP servers under Settings → MCP servers; each member connects their own account once (Connect or Set key), and the server holds that credential for every device. Pick the servers a session gets in the composer's + menu. On a headless box: \`exponential mcp list|connect|set-secret|disconnect\` (${link(`/docs/build/cli/`, `CLI`)}).

`

/* ── The list ── */
const generated = (entry: Omit<ReferenceSource, `transform`>, render: () => string): ReferenceSource => ({
  ...entry,
  transform: () => render(),
})

const CHANGELOG_PAGE = await changelog()

export const REFERENCE: ReferenceSource[] = [
  {
    source: `INSTALL.md`,
    slug: `docs/get-started/self-host`,
    title: `Self-host`,
    description: `Run Exponential on your own server from the published Docker images: install, configure, upgrade, back up.`,
  },
  {
    source: `packages/exponential-ui/docs/package-format.md`,
    slug: `docs/sdk/apps/reference`,
    title: `Package reference`,
    description: `The Apps package format in full: versions, records, expressions, actions, tests, tools, exposes, extensions, the file, and the limits.`,
    transform: (md) => `${md.trimEnd()}\n\n${eappsLimits()}`,
  },
  ...SPEC_FILES.map((s) => ({ ...s, transform: s.source === `docs/spec/apps.md` ? specImages : undefined })),
  generated(
    {
      source: `docs/spec/apps.md`,
      slug: `docs/reference/spec`,
      title: `Specs`,
      description: `The design specs behind Exponential and its SDK, generated from the repo.`,
    },
    specIndex
  ),
  generated(
    {
      source: `apps/web/src/lib/changelog.ts`,
      slug: `docs/reference/changelog`,
      title: `Changelog`,
      description: `Every user-facing release of Exponential, newest first.`,
    },
    () => CHANGELOG_PAGE
  ),
  generated(
    {
      source: `${CLI}/main.rs`,
      slug: `docs/build/cli`,
      title: `CLI`,
      description: `Install the exponential CLI, sign in, run the daemon, and every command with its options.`,
    },
    cli
  ),
  generated(
    {
      source: `apps/web/src/lib/emcp/nouns.ts`,
      slug: `docs/build/mcp`,
      title: `MCP tools`,
      description: `Connect an MCP client to Exponential, and every tool and op with its parameters.`,
    },
    () => MCP_GUIDE + mcpReference()
  ),
]
