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
  { id: `your-own-servers`, num: `06`, label: `Your own servers` },
]

/* Tool reference — the 16 noun tools (NEW-4), mirroring apps/web/src/lib/mcp/nouns.ts:
   every tool takes `op` plus that op's fields; its description lists every op
   with its parameters. */
const TOOL_GROUPS: {
  heading: string
  tools: { name: string; desc: string }[]
}[] = [
  {
    heading: `Work`,
    tools: [
      { name: `exponential_issues`, desc: `op list (filters by boards, status / statusId / statusCategory each with an exclude twin, priority, assignee, labels any/all/unlabeled, source, comment activity, created/updated/due ranges, text and identifier search, sort; open work only unless includeClosed or a status filter), get (by UUID or identifier "EXP-42", with label ids, relations, latest comments), show (the interactive list in MCP Apps hosts), create, update, delete, activity, update_status, subscribe, unsubscribe, labels_add, labels_remove, relations_add, relations_remove.` },
      { name: `exponential_comments`, desc: `op list, create, update, delete. Plain GFM; threads one level deep; audience "reporter" emails a widget reporter.` },
      { name: `exponential_attachments`, desc: `op list, get (metadata plus contents or a signed URL), upload (bytes now, or a signed uploadUrl then a finalize call), delete.` },
      { name: `exponential_pr`, desc: `op open (links the session's issues, infers the base in a stack), merge (squash through the GitHub App; stack: true lands a stack member and everything beneath it), close, retarget, update, unlink, files. Name a PR by pullRequestId, issueId or repositoryId + prNumber; default = the session's newest open PR.` },
    ],
  },
  {
    heading: `Teams and boards`,
    tools: [
      { name: `exponential_teams`, desc: `op list, get, create, update.` },
      { name: `exponential_boards`, desc: `op list, get, create, update, delete (the 48-hour trash), set_repository.` },
      { name: `exponential_people`, desc: `op list and add members; invites_list, invites_create, invites_revoke.` },
      { name: `exponential_team_config`, desc: `op labels_list/get/create/update/delete, statuses_list/create/update/delete, prompts_list/get/create/update/delete, agents_list, mcp_servers_list/add/remove, repositories_list/add.` },
      { name: `exponential_apps`, desc: `op list, get (the head or a version's package), upsert (publishes a new version; a new slug creates the app), delete, catalog (the components, bindings, functions and records a package may use), records_list, records_upsert, records_delete (an app's own data).` },
    ],
  },
  {
    heading: `Sessions and agents`,
    tools: [
      { name: `exponential_sessions_send`, desc: `THE session verb, no op: sessionId messages a session (answer answers its open question or plan; stop: true interrupts its turn; issueIds forwards issues; account switches its login; deviceId moves it), agentId messages a team agent (your own conversation with it), appId your app editor session, parentId starts a child, neither starts a thread (text, issueIds, promptId, repositoryId; deviceId required). Every transition of a session comes back as a message.` },
      { name: `exponential_sessions`, desc: `op get (a session tree: title, issues, PRs, children, latest messages; no id = your own), list, link, unlink, end.` },
      { name: `exponential_session_self`, desc: `Inside a session only: op suggest (one-tap replies or follow-up tasks), compact (ask the host to compact your context), report_bug (when Exponential itself misbehaves; cloud only).` },
      { name: `exponential_show`, desc: `Inside a session only, no op: one card in the chat per turn (an Exponential UI surface, a question with options, or a plan with questions; replace: true updates it). Questions, plans and cards with pictures are stories: they land in the Stories rings on top of Chats, and the pictures (labels starting web, ios, android or desktop, each uploaded with the returned curl line) are what a PR body shows.` },
      { name: `exponential_devices`, desc: `op list (the machines running sessions, their agents and logins), account_login.` },
      { name: `exponential_events`, desc: `op emit (a named event with a payload; every agent with a trigger on that name wakes), list.` },
      { name: `exponential_notifications`, desc: `op list, mark_read.` },
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
              issues: file bugs from chat, triage from your editor, script
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
              Self-hosting? It&apos;s the same path on your instance:{` `}
              <code>https://your-instance/api/mcp</code>. There is no separate
              {` `}
              <code>/sse</code> variant; modern clients speak streamable HTTP
              directly.
            </p>
          </DocsSection>

          {/* ── 02 Authentication ── */}
          <DocsSection id="auth" num="02" label="Authentication">
            <h2>Authentication</h2>
            <h3>OAuth for interactive clients</h3>
            <p>
              Point a client at the endpoint with no credentials and it
              registers itself (dynamic client registration) and sends you to
              your browser to approve. The consent screen is a{` `}
              <strong>scope picker</strong>: grant the client{` `}
              <em>everything</em>, specific teams, or specific boards. The
              token it receives is confined to exactly that grant. A client
              with no grant gets nothing. Re-running consent updates the
              grant, so you can widen or narrow access later.
            </p>
            <h3>API keys for headless use</h3>
            <p>
              Scripts and CI use a personal API key instead. Generate one
              under <strong>Settings → Security</strong> in the web app and
              send it as a bearer token:
            </p>
            <DocsCode language="text">{`Authorization: Bearer expu_...`}</DocsCode>
            <p>
              A key acts as you. Minted without a scope it carries your full
              membership; the mint dialog offers the same team/board scope
              picker as the consent screen, and a scoped key is confined to
              that pick and works for MCP only. Guard keys accordingly.
            </p>
          </DocsSection>

          {/* ── 03 Client setup ── */}
          <DocsSection id="clients" num="03" label="Client setup">
            <h2>Client setup</h2>

            <h3>Claude (Desktop &amp; claude.ai)</h3>
            <p>
              <strong>Settings → Connectors → Add custom connector</strong>,
              paste the endpoint URL, and hit <strong>Connect</strong>. Your
              browser opens for OAuth and the scope picker. Works the same in
              the Claude desktop app and on claude.ai.
            </p>
            <DocsCallout kind="note" title="Connectors dial from Anthropic's cloud">
              Claude&apos;s connectors connect server-side, so a self-hosted
              instance must be reachable from the internet. A LAN-only
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

            <h3>OpenClaw</h3>
            <p>
              Install the Exponential plugin from ClawHub (the MCP server plus
              a skill) and give the gateway an API key, or add the server by
              hand and sign in with OAuth:
            </p>
            <DocsCode language="shell">{`
openclaw plugins install clawhub:@exponential/openclaw-plugin
export EXPONENTIAL_API_KEY=expu_...

# or, with OAuth
openclaw mcp add exponential --url ${LINKS.app.mcp} --transport streamable-http --auth oauth
openclaw mcp login exponential
`}</DocsCode>
            <p>
              Turn on OpenClaw&apos;s MCP Apps bridge (
              <code>openclaw config set mcp.apps.enabled true --strict-json</code>
              ) and the issue list, your inbox and your devices render as
              Exponential&apos;s own views in the dashboard; the issue list
              also opens straight from the app catalog.
            </p>

            <h3>Interactive views (MCP Apps)</h3>
            <p>
              Clients that render MCP Apps show three tools as Exponential
              views instead of JSON: <code>exponential_issues</code> op show (the
              issue list, grouped by status; a row opens the issue),{` `}
              <code>exponential_notifications</code> op list (your inbox) and
              {` `}<code>exponential_devices</code> op list (your machines and
              their agent logins). The session tools answer with JSON. Self-hosted instances serve the same
              views; nothing to configure on the server.
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
              16 tools, one per noun. Every call carries an <code>op</code>
              and that op&apos;s fields beside it, for example{` `}
              <code>exponential_issues {`{op: "get", id: "EXP-42"}`}</code>;
              a tool&apos;s description lists every op with its parameters.
              Every call is confined to the OAuth grant&apos;s scope (or the
              API key&apos;s membership or scope). List ops paginate (50 by default, 200
              at most, 1000 for issues; comments list 100 by default), and issue parameters take an
              identifier (&quot;EXP-42&quot;) wherever they take a UUID.
            </p>
            <DocsCallout kind="note" title="The same tools for every caller">
              Every client sees all 16. An op that only makes sense inside an
              agent session the launcher started (show, the self ops) or
              on the cloud (report_bug) answers with a message saying so
              instead of hiding.
            </DocsCallout>
            {TOOL_GROUPS.map((group) => (
              <div key={group.heading}>
                <h3>{group.heading}</h3>
                <ul>
                  {group.tools.map((tool) => (
                    <li key={tool.name}>
                      <code>{tool.name}</code>: {tool.desc}
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
              &quot;File a bug on the app board: it drops drag events
              on narrow viewports. Priority high, label it{` `}
              <em>bug</em>.&quot; The client chains{` `}
              <code>exponential_boards</code> op list →{` `}
              <code>exponential_issues</code> op create →{` `}
              <code>exponential_team_config</code> op labels_list →{` `}
              <code>exponential_issues</code> op labels_add, and answers with the
              new identifier.
            </p>

            <h3>Check a PR&apos;s files from chat</h3>
            <p>
              &quot;What does EXP-42&apos;s PR actually change?&quot; The client
              calls <code>exponential_pr</code> op files, which returns the
              changed files with patches, so the model can summarize the diff,
              flag a risky change, or compare it against the issue&apos;s
              acceptance criteria.
            </p>

            <h3>One combined PR for several issues</h3>
            <p>
              An agent that fixed several issues on one pushed branch opens a
              single PR for all of them:{` `}
              <code>exponential_pr</code> op open with{` `}
              <code>issueIds</code> (the batch) and <code>head</code> (the
              pushed branch). Every listed issue links to the PR and moves to
              In Review; merging completes them all. This is what a
              {` `}
              <a href="/docs/coding/#batch-runs">session on several issues</a> does.
            </p>

            <h3>A pull request with no issue behind it</h3>
            <p>
              Some work never gets filed: a dependency bump, a typo sweep. Pass
              {` `}
              <code>repositoryId</code> + <code>head</code> to{` `}
              <code>exponential_pr</code> op open (and{` `}
              <code>repositoryId</code> + <code>prNumber</code> to{` `}
              <code>exponential_pr</code> op merge) and you get an ordinary pull
              request with nothing linked, moved, or notified.
            </p>

            <h3>Start a session on one of your machines, from chat</h3>
            <p>
              &quot;Have my build box take EXP-42.&quot; The client calls{` `}
              <code>exponential_devices</code> op list to find an online machine
              that runs the agent you want, then{` `}
              <code>exponential_sessions_send</code>, and follows the session
              with <code>exponential_sessions</code> op get. Steer it mid-session
              with <code>exponential_sessions_send</code> (sessionId) and read its
              replies with <code>exponential_sessions</code> op get. Details
              in{` `}
              <a href="/docs/cli/#daemon">CLI &amp; daemon</a>.
            </p>
          </DocsSection>

          {/* ── 06 Your own servers ── */}
          <DocsSection
            id="your-own-servers"
            num="06"
            label="Your own servers"
          >
            <h2>Your own MCP servers</h2>
            <p>
              Everything above is Exponential <em>as</em> an MCP server. This
              is the other direction: the MCP servers <em>your</em> coding
              sessions connect to besides Exponential — your error tracker, your
              docs, your internal API.
            </p>
            <p>
              They are a team registry under{` `}
              <strong>Settings → MCP servers</strong>. Every member sees the
              list; owners add a server from a catalog or a pasted URL (the
              technical fields sit behind <strong>Advanced</strong>) and edit
              or remove it.
            </p>
            <DocsCallout kind="note" title="Each member connects once">
              Every member connects their <em>own</em> account to a server
              once, with the row&apos;s action: <strong>Connect</strong> for an
              OAuth server, <strong>Set key</strong> for one that takes a key.
              The server holds that credential for you, so it works on every
              one of your devices, remote starts and triggers; no machine
              stores it.
            </DocsCallout>
            <p>
              On a headless box,{` `}
              <code>exponential mcp list</code> shows your connection to each
              server, <code>exponential mcp connect &lt;server&gt;</code>
              {` `}opens the web connect page in your signed-in browser,{` `}
              <code>exponential mcp set-secret &lt;server&gt;</code> reads a
              key from a no-echo prompt or stdin (never a command line), and
              {` `}<code>exponential mcp disconnect &lt;server&gt;</code>
              {` `}deletes your credential.
            </p>
            <p>
              Pick the servers a session gets under{` `}
              <strong>MCP servers</strong> in the composer&apos;s{` `}
              <a href="/docs/coding/#start-coding">+ menu</a>; a server you have
              not connected opens Settings → MCP servers instead. Sessions
              are strict about it: an agent started by Exponential connects to
              the servers Exponential wired in and nothing else, whatever the
              CLI has configured globally.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
