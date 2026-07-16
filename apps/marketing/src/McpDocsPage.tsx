import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `endpoint`, num: `01`, label: `The endpoint` },
  { id: `auth`, num: `02`, label: `Authentication` },
  { id: `clients`, num: `03`, label: `Client setup` },
  { id: `tools`, num: `04`, label: `Tool reference` },
  { id: `recipes`, num: `05`, label: `Recipes` },
]

/* Tool reference — names + one-liners mirroring apps/web/src/lib/mcp/tools.ts. */
const TOOL_GROUPS: {
  heading: string
  tools: { name: string; desc: string }[]
}[] = [
  {
    heading: `Workspaces (teams)`,
    tools: [
      { name: `exponential_workspaces_list`, desc: `List the teams you belong to.` },
      { name: `exponential_workspaces_get`, desc: `Get a single team by id.` },
      { name: `exponential_workspaces_create`, desc: `Create a new team you own.` },
      { name: `exponential_workspaces_update`, desc: `Rename a team or change its icon (owner only).` },
    ],
  },
  {
    heading: `Projects`,
    tools: [
      { name: `exponential_projects_list`, desc: `List projects in one team or across all your teams.` },
      { name: `exponential_projects_get`, desc: `Get a single project.` },
      { name: `exponential_projects_create`, desc: `Create a project — optionally a public feedback board, optionally repo-backed.` },
      { name: `exponential_projects_update`, desc: `Update name, color, icon, publicness, public-board toggles, or archive state.` },
      { name: `exponential_projects_delete`, desc: `Move a project to the 48-hour trash (owner only).` },
      { name: `exponential_projects_set_repository`, desc: `Point a project at a different registered repository.` },
    ],
  },
  {
    heading: `Issues`,
    tools: [
      { name: `exponential_issues_list`, desc: `List and filter issues — project, status, priority, assignee, due dates, title search.` },
      { name: `exponential_issues_get`, desc: `Get one issue with labels and recent comments, by UUID or identifier ("EXP-42").` },
      { name: `exponential_issues_create`, desc: `Create an issue.` },
      { name: `exponential_issues_update`, desc: `Update an issue's fields — pass only what changes.` },
      { name: `exponential_issues_delete`, desc: `Permanently delete an issue and everything attached to it.` },
      { name: `exponential_issues_update_status`, desc: `Set status during a coding run — in_progress or done (pr_open handles in_review).` },
      { name: `exponential_pr_open`, desc: `Open + link the pull request server-side — one issue, or a whole batch via issueIds + head.` },
      { name: `exponential_issues_pr_files`, desc: `List the linked PR's changed files with patches and add/delete counts.` },
    ],
  },
  {
    heading: `Labels & issue labels`,
    tools: [
      { name: `exponential_labels_list`, desc: `List a team's labels.` },
      { name: `exponential_labels_get`, desc: `Get a label by id.` },
      { name: `exponential_labels_create`, desc: `Create a label.` },
      { name: `exponential_labels_update`, desc: `Rename or recolor a label.` },
      { name: `exponential_labels_delete`, desc: `Delete a label.` },
      { name: `exponential_issue_labels_add`, desc: `Attach a label to an issue.` },
      { name: `exponential_issue_labels_remove`, desc: `Detach a label from an issue.` },
    ],
  },
  {
    heading: `Comments`,
    tools: [
      { name: `exponential_comments_list`, desc: `List an issue's comments, oldest first.` },
      { name: `exponential_comments_create`, desc: `Post a comment as the connected user.` },
      { name: `exponential_comments_update`, desc: `Edit your own comment.` },
      { name: `exponential_comments_delete`, desc: `Delete a comment.` },
    ],
  },
  {
    heading: `Subscriptions & notifications`,
    tools: [
      { name: `exponential_issues_subscribe`, desc: `Subscribe to an issue's notifications.` },
      { name: `exponential_issues_unsubscribe`, desc: `Unsubscribe (and suppress auto-resubscribe).` },
      { name: `exponential_notifications_list`, desc: `List your notifications, newest first.` },
      { name: `exponential_notifications_mark_read`, desc: `Mark one notification read — or all of them.` },
    ],
  },
  {
    heading: `Members & invites`,
    tools: [
      { name: `exponential_members_list`, desc: `List a team's members — useful to resolve an assigneeId.` },
      { name: `exponential_invites_create`, desc: `Create an invite link (owner only).` },
      { name: `exponential_invites_list`, desc: `List pending invites.` },
      { name: `exponential_invites_revoke`, desc: `Revoke a pending invite (owner only).` },
    ],
  },
  {
    heading: `Repositories & branch diff`,
    tools: [
      { name: `exponential_repositories_list`, desc: `List a team's registered repositories and the projects they back.` },
      { name: `exponential_repositories_add`, desc: `Register a GitHub repository ("owner/name") with a team.` },
      { name: `exponential_repositories_branch_diff`, desc: `Diff an issue's branch against the repo's default branch.` },
    ],
  },
  {
    heading: `Run configs`,
    tools: [
      { name: `exponential_run_configs_list`, desc: `List a project's run configs.` },
      { name: `exponential_run_configs_create`, desc: `Create a named run config — argv, cwd, env (owner only).` },
      { name: `exponential_run_configs_update`, desc: `Update a run config (owner only).` },
      { name: `exponential_run_configs_delete`, desc: `Delete a run config (owner only).` },
    ],
  },
  {
    heading: `Attachments`,
    tools: [
      { name: `exponential_attachments_get`, desc: `Fetch an image attachment so the client can view it.` },
      { name: `exponential_attachments_upload`, desc: `Upload an image and get its embeddable markdown form back.` },
    ],
  },
]

export function McpDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>MCP &amp; API</h1>
            <p>
              Connect Claude, ChatGPT, Cursor, or any MCP client to your
              issues — file bugs from chat, triage from your editor, script
              your board.
            </p>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/mcp/">
          {/* ── 01 The endpoint ── */}
          <DocsSection id="endpoint" num="01" label="The endpoint">
            <h2>The endpoint</h2>
            <p>
              Every Exponential instance exposes a{` `}
              <strong>streamable-HTTP MCP server</strong> at:
            </p>
            <DocsCode language="text">{LINKS.app.mcp}</DocsCode>
            <p>
              Self-hosting? It&apos;s the same path on your instance —{` `}
              <code>https://your-instance/api/mcp</code>. There is no separate
              {` `}
              <code>/sse</code> variant; modern clients speak streamable HTTP
              directly.
            </p>
          </DocsSection>

          {/* ── 02 Authentication ── */}
          <DocsSection id="auth" num="02" label="Authentication">
            <h2>Authentication</h2>
            <h3>OAuth — for interactive clients</h3>
            <p>
              Point a client at the endpoint with no credentials and it
              registers itself (dynamic client registration) and sends you to
              your browser to approve. The consent screen is a{` `}
              <strong>scope picker</strong>: grant the client{` `}
              <em>everything</em>, specific teams, or specific projects. The
              token it receives is confined to exactly that grant — a client
              with no grant gets nothing. Re-running consent updates the
              grant, so you can widen or narrow access later.
            </p>
            <h3>API key — for headless use</h3>
            <p>
              Scripts and CI use a personal API key instead — create one in
              your account settings and send it as a bearer token:
            </p>
            <DocsCode language="text">{`Authorization: Bearer expu_...`}</DocsCode>
            <p>
              API keys act as you, with your full membership — guard them
              accordingly.
            </p>
          </DocsSection>

          {/* ── 03 Client setup ── */}
          <DocsSection id="clients" num="03" label="Client setup">
            <h2>Client setup</h2>

            <h3>Claude (Desktop &amp; claude.ai)</h3>
            <p>
              <strong>Settings → Connectors → Add custom connector</strong>,
              paste the endpoint URL, and hit <strong>Connect</strong> — your
              browser opens for OAuth and the scope picker. Works the same in
              the Claude desktop app and on claude.ai.
            </p>
            <DocsCallout kind="note" title="Connectors dial from Anthropic's cloud">
              Claude&apos;s connectors connect server-side, so a self-hosted
              instance must be reachable from the internet — a LAN-only
              instance won&apos;t work here. Connectors are OAuth-only; to use
              an API key instead, bridge through{` `}
              <code>mcp-remote</code> (see &quot;Other clients&quot; below).
            </DocsCallout>

            <h3>ChatGPT</h3>
            <p>
              On chatgpt.com, enable{` `}
              <strong>
                Settings → Apps &amp; Connectors → Advanced settings →
                Developer mode
              </strong>{` `}
              (Plus/Pro/Business; the menu naming varies while the feature is
              in beta). Then <strong>Create connector</strong>, paste the MCP
              URL, and choose <strong>OAuth</strong> as the authentication.
              Write-capable tools ask for confirmation per call.
            </p>

            <h3>Claude Code</h3>
            <DocsCode language="shell">{`
claude mcp add --transport http --scope user exponential ${LINKS.app.mcp}
`}</DocsCode>
            <p>
              Then run <code>/mcp</code> in a session to sign in via OAuth.
              For headless use, attach an API key instead:
            </p>
            <DocsCode language="shell">{`
claude mcp add --transport http --scope user exponential ${LINKS.app.mcp} \\
  --header "Authorization: Bearer expu_..."
`}</DocsCode>
            <p>
              A configured header disables the OAuth fallback. In{` `}
              <code>.mcp.json</code> form, the server entry needs{` `}
              <code>&quot;type&quot;: &quot;http&quot;</code>.
            </p>

            <h3>Codex CLI</h3>
            <DocsCode language="shell">{`
codex mcp add exponential --url ${LINKS.app.mcp}
codex mcp login exponential
`}</DocsCode>
            <p>
              Or configure it in <code>config.toml</code>, with an API key via
              an environment variable:
            </p>
            <DocsCode language="toml">{`
[mcp_servers.exponential]
url = "${LINKS.app.mcp}"
bearer_token_env_var = "EXPONENTIAL_API_KEY"
`}</DocsCode>

            <h3>Cursor</h3>
            <p>
              Add the server to <code>~/.cursor/mcp.json</code> (or a
              project&apos;s <code>.cursor/mcp.json</code>):
            </p>
            <DocsCode language="json">{`
{
  "mcpServers": {
    "exponential": {
      "url": "${LINKS.app.mcp}"
    }
  }
}
`}</DocsCode>
            <p>
              Click <strong>Connect</strong> next to the server in
              Cursor&apos;s MCP list to run the OAuth flow. For API keys, add
              a <code>headers</code> object with the{` `}
              <code>Authorization</code> header instead.
            </p>

            <h3>Other clients</h3>
            <p>
              Most MCP clients accept the generic <code>mcpServers</code> JSON
              shown above. VS Code (Copilot) uses a top-level{` `}
              <code>servers</code> key with{` `}
              <code>&quot;type&quot;: &quot;http&quot;</code> per server.
              Clients that only speak stdio can bridge:
            </p>
            <DocsCode language="shell">{`
npx mcp-remote ${LINKS.app.mcp}
`}</DocsCode>
          </DocsSection>

          {/* ── 04 Tool reference ── */}
          <DocsSection id="tools" num="04" label="Tool reference">
            <h2>Tool reference</h2>
            <p>
              What a connected client can do, grouped by area. Tool names use
              {` `}
              <code>workspace</code> — the API vocabulary for what the UI
              calls a team. Every call is confined to the OAuth grant&apos;s
              scope (or the API key&apos;s membership).
            </p>
            {TOOL_GROUPS.map((group) => (
              <div key={group.heading}>
                <h3>{group.heading}</h3>
                <ul>
                  {group.tools.map((tool) => (
                    <li key={tool.name}>
                      <code>{tool.name}</code> — {tool.desc}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </DocsSection>

          {/* ── 05 Recipes ── */}
          <DocsSection id="recipes" num="05" label="Recipes">
            <h2>Recipes</h2>

            <h3>File a bug with labels, from chat</h3>
            <p>
              &quot;File a bug in the app project: the board drops drag events
              on narrow viewports. Priority high, label it{` `}
              <em>bug</em>.&quot; The client chains{` `}
              <code>exponential_projects_list</code> →{` `}
              <code>exponential_issues_create</code> →{` `}
              <code>exponential_labels_list</code> →{` `}
              <code>exponential_issue_labels_add</code> — and answers with the
              new identifier.
            </p>

            <h3>Check a PR&apos;s files from chat</h3>
            <p>
              &quot;What does EXP-42&apos;s PR actually change?&quot; —{` `}
              <code>exponential_issues_pr_files</code> returns the changed
              files with patches, so the model can summarize the diff, flag a
              risky change, or compare it against the issue&apos;s acceptance
              criteria.
            </p>

            <h3>One combined PR for several issues</h3>
            <p>
              An agent that fixed several issues on one pushed branch opens a
              single PR for all of them:{` `}
              <code>exponential_pr_open</code> with{` `}
              <code>issueIds</code> (the batch) and <code>head</code> (the
              pushed branch). Every listed issue links to the PR and moves to
              In Review; merging completes them all. This is exactly what a
              {` `}
              <a href="/docs/coding/#batch-runs">batch coding run</a> does.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
