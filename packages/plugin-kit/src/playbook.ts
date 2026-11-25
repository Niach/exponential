import { readFileSync } from "node:fs"
import { MCP_ENTRY, MCP_URL } from "./mcp-entry"

// #NEW-23: THE agent playbook. `playbook/core.md` is the one body; a host
// adds its frontmatter description, its name, and the few paragraphs that
// only it can say (how its tools are named, what a failed sign-in means,
// what the host itself draws). Every `SKILL.md` under `integrations/` is this
// rendered; nobody edits one.

export type PlaybookHost = `claude` | `claude-code` | `openclaw`

interface HostPlaybook {
  /** Repo-relative output. */
  path: string
  description: string
  /** How the body names the host ("where Claude renders MCP Apps"). */
  host: string
  /** What reaches Exponential here: a connector, an MCP server. */
  connection: string
  slots: Record<`tool-names` | `sign-in-failure` | `host-section`, string>
  /** Whether the adapter ships the SDK skills the body points at. */
  sdkSkills: boolean
}

const SDK_SKILL_POINTERS = {
  "genui-skill": `, and\nthe \`exponential-genui\` skill covers the format`,
  "apps-skill": `; the\n\`exponential-apps\` skill covers the package format`,
}

const CONNECTOR_SIGN_IN = `- A 401 or a sign-in prompt: the connector is not signed in or the grant was
  revoked; ask the person to reconnect instead of retrying.`

export const PLAYBOOKS: Record<PlaybookHost, HostPlaybook> = {
  claude: {
    path: `integrations/claude/skills/exponential/SKILL.md`,
    description: `Work in Exponential (exponential.at), the realtime issue tracker with coding agents: find, show, create and update issues on boards, comment, follow pull requests and sessions, open the interactive issue list and inbox, and start a coding agent on one of the person's devices. Use when the person mentions Exponential, an issue identifier like EXP-12, their boards, or asks to track or hand off work there.`,
    host: `Claude`,
    connection: `connector`,
    sdkSkills: true,
    slots: {
      "tool-names": `In Claude Code the tools carry a plugin prefix
(\`mcp__plugin_exponential_exponential__exponential_issues\`); the names above
are the part after the last \`__\`.`,
      "sign-in-failure": CONNECTOR_SIGN_IN,
      "host-section": ``,
    },
  },
  "claude-code": {
    path: `integrations/claude-code/skills/exponential/SKILL.md`,
    description: `Work in Exponential (exponential.at), the realtime issue tracker with coding agents, from Claude Code: find, create and update issues on boards, comment, follow the pull request and the sessions of the issue you are working on, and start a coding agent on one of the person's devices. Use when the person mentions Exponential, an issue identifier like EXP-12, their boards, or asks to track or hand off work there.`,
    host: `Claude Code`,
    connection: `connector`,
    sdkSkills: false,
    slots: {
      "tool-names": `Here the tools carry the plugin's prefix (\`mcp__plugin_…__exponential_issues\`);
the names above are the part after the last \`__\`.`,
      "sign-in-failure": CONNECTOR_SIGN_IN,
      "host-section": `
## In Claude Code

Claude Code draws no MCP Apps views, so \`op: "show"\` answers its text summary
here. The plugin draws the same data itself, and the person sees it without
asking you:

- a pane with the current issue (the one the branch names, or the one they
  pinned), its pull request and its live sessions: \`/exp-issues\` opens it on
  the open-issues list, \`/exp-issue EXP-12\` on one issue;
- a status line with the issue and where its pull request stands;
- a toast when a comment, a mention or a waiting session arrives.

So do not repeat what the pane shows unless asked. \`/exp-start EXP-12\` hands
you the issue to start a coding agent on: follow "Coding agents on the
person's devices" above.`,
    },
  },
  openclaw: {
    path: `integrations/openclaw/skills/exponential/SKILL.md`,
    description: `Track and change work in Exponential (exponential.at): list, show, create and update issues on boards, comment, follow pull requests and sessions, message team agents, and show the interactive issue list.`,
    host: `OpenClaw`,
    connection: `MCP server`,
    sdkSkills: false,
    slots: {
      "tool-names": `Here the tools appear as \`exponential__exponential_*\`; the names above are the
part after the \`__\`.`,
      "sign-in-failure": `- A 401: neither the \`${MCP_ENTRY.apiKeyEnv}\` environment variable (a personal
  \`${MCP_ENTRY.apiKeyPrefix}\` key from Settings → Security) nor an OAuth login is set; say so
  instead of retrying.`,
      "host-section": `
## Connection

The server is \`${MCP_URL}\` (self-hosted: the instance's
\`${MCP_ENTRY.path}\`). It authenticates with the \`${MCP_ENTRY.apiKeyEnv}\` environment
variable or, when the operator configured \`auth: "oauth"\`, with OAuth. A view
lives as long as OpenClaw's view lease; an older card is read-only until it is
relaunched from the chat.`,
    },
  },
}

const CORE = new URL(`./playbook/core.md`, import.meta.url)

export function playbookCore(): string {
  return readFileSync(CORE, `utf8`)
}

/** A host's `SKILL.md`. An empty slot takes its line with it. */
export function renderPlaybook(host: PlaybookHost, core = playbookCore()): string {
  const playbook = PLAYBOOKS[host]
  const body = core
    .replace(/^\{\{> ([a-z-]+)\}\}\n/gm, (_, name: keyof HostPlaybook[`slots`]) => {
      const slot = playbook.slots[name]
      if (slot === undefined) throw new Error(`playbook: unknown slot ${name}`)
      return slot ? `${slot}\n` : ``
    })
    .replaceAll(`{{genui-skill}}`, playbook.sdkSkills ? SDK_SKILL_POINTERS[`genui-skill`] : ``)
    .replaceAll(`{{apps-skill}}`, playbook.sdkSkills ? SDK_SKILL_POINTERS[`apps-skill`] : ``)
    .replaceAll(`{{host}}`, playbook.host)
    .replaceAll(`{{connection}}`, playbook.connection)
  if (/\{\{/.test(body)) throw new Error(`playbook: an unfilled token in ${host}`)
  return `---\nname: exponential\ndescription: ${JSON.stringify(playbook.description)}\n---\n\n${body}`
}
