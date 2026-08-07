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
  { id: `install`, num: `01`, label: `Install` },
  { id: `sign-in`, num: `02`, label: `Sign in` },
  { id: `commands`, num: `03`, label: `Commands` },
  { id: `daemon`, num: `04`, label: `Run as a daemon` },
  { id: `updating`, num: `05`, label: `Updating` },
]

/* Command reference — mirroring the binary's own usage text
   (apps/desktop/crates/cli/src/main.rs). */
const COMMANDS: { name: string; desc: string }[] = [
  { name: `exponential login [--instance <url>]`, desc: `Sign in to an instance and store the session locally.` },
  { name: `exponential logout`, desc: `Sign out and drop the local credentials.` },
  { name: `exponential whoami`, desc: `Show the signed-in account and its instance.` },
  { name: `exponential status`, desc: `Account, device id, daemon state, installed agents, and git in one summary.` },
  { name: `exponential doctor`, desc: `Check git and the three agent CLIs, and say what's missing.` },
  { name: `exponential code <ISSUE> [--agent claude|codex|pi] [--model <m>] [--effort <e>] [--plan] [--skip-permissions] [--detach]`, desc: `Start a coding session for an issue, by identifier ("EXP-42") or id.` },
  { name: `exponential run <action> [--team <id>] [--input k=v ...]`, desc: `Run a team action by name or id — the same agent flags apply.` },
  { name: `exponential daemon [--foreground] [--label <name>]`, desc: `Run the remote-start daemon in this terminal.` },
  { name: `exponential daemon install | uninstall | status`, desc: `Manage the systemd user unit (Linux) or launchd agent (macOS).` },
  { name: `exponential update`, desc: `Self-update from the latest CLI release.` },
  { name: `exponential uninstall [--yes]`, desc: `Remove the daemon service and delete the binary. Signed-in accounts are kept — the data dir is shared with the desktop app.` },
  { name: `exponential version`, desc: `Print the CLI version.` },
]

export function CliDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>CLI &amp; daemon</h1>
            <p>
              Start coding sessions and run actions from any terminal — and
              leave a Linux or macOS box running as an always-on agent
              machine your team starts work on from the web.
            </p>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/cli/">
          {/* ── 01 Install ── */}
          <DocsSection id="install" num="01" label="Install">
            <h2>Install</h2>
            <DocsCode language="shell">{`
curl -fsSL https://exponential.at/install.sh | sh
`}</DocsCode>
            <p>
              Self-hosting? Same script, with your instance in{` `}
              <code>EXP_INSTANCE</code>:
            </p>
            <DocsCode language="shell">{`
curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://issues.example.com sh
`}</DocsCode>
            <p>
              Builds are published for <strong>Linux</strong> (x86_64 and
              arm64) and <strong>macOS</strong> (Apple Silicon). The script
              verifies the binary&apos;s checksum and installs it to{` `}
              <code>~/.local/bin/exponential</code>, then hands straight over
              to <code>exponential login</code> when it has a terminal.
            </p>
            <p>
              You also need <code>git</code>, plus at least one agent CLI —{` `}
              <strong>Claude Code</strong>, <strong>Codex</strong>, or{` `}
              <strong>pi</strong> — for coding sessions. See{` `}
              <a href="/docs/coding/">Coding agents</a> for what each one
              supports. The installer only warns about missing tools;{` `}
              <code>exponential doctor</code> is the one that checks them
              properly.
            </p>
            <DocsCallout kind="note" title="~/.local/bin on your PATH">
              The script says so if it isn&apos;t. Add it with{` `}
              <code>export PATH=&quot;$HOME/.local/bin:$PATH&quot;</code> in
              your shell profile, or set <code>EXP_INSTALL_DIR</code> to
              install somewhere else.
            </DocsCallout>
          </DocsSection>

          {/* ── 02 Sign in ── */}
          <DocsSection id="sign-in" num="02" label="Sign in">
            <h2>Sign in</h2>
            <DocsCode language="shell">{`
exponential login
`}</DocsCode>
            <p>
              This is a <strong>device code</strong> flow (RFC 8628). The CLI
              prints a short code and a URL —{` `}
              <code>&lt;instance&gt;/auth/device</code> — you open it in any
              browser where you&apos;re already signed in, type the code, and
              approve. The terminal picks up the session by itself.
            </p>
            <p>
              Nothing has to be typed into the machine you&apos;re installing
              on, so this works the same over SSH on a headless server as it
              does locally, and it works for password and SSO accounts alike.
              Confirm with <code>exponential whoami</code>.
            </p>
            <h3>Non-interactive setups</h3>
            <p>
              For provisioning scripts, skip the browser: generate an API key
              under <strong>Settings → API keys</strong> in the web app and
              hand it to the CLI as <code>EXP_TOKEN</code>:
            </p>
            <DocsCode language="shell">{`
EXP_INSTANCE=https://issues.example.com EXP_TOKEN=expu_... exponential login
`}</DocsCode>
          </DocsSection>

          {/* ── 03 Commands ── */}
          <DocsSection id="commands" num="03" label="Commands">
            <h2>Commands</h2>
            <ul>
              {COMMANDS.map((command) => (
                <li key={command.name}>
                  <code>{command.name}</code>: {command.desc}
                </li>
              ))}
            </ul>
            <p>
              <code>code</code> and <code>run</code> take the same agent
              options as the desktop&apos;s start-coding dialog, and fall
              back to your saved per-agent defaults when you omit them.{` `}
              <code>--plan</code> and <code>--skip-permissions</code> are
              agent-specific; see <a href="/docs/coding/">Coding agents</a>.
            </p>
            <p>
              Run <code>code</code> from a terminal and the agent&apos;s
              interactive TUI attaches to it — type at it exactly like you
              would in the desktop app. Without a terminal (in CI, over a
              pipe) or with <code>--detach</code>, the session runs headless
              and stays fully steerable from the web, so you can close the
              laptop and keep watching it from your phone.
            </p>
            <DocsCode language="shell">{`
exponential code EXP-42 --agent claude --plan
exponential run "Update the changelog" --input version=1.4.0
exponential run fix-conflicts --input pr=EXP-42
`}</DocsCode>
            <p>
              Either way it&apos;s a real{` `}
              <a href="/docs/coding/">coding session</a>: a worktree on an{` `}
              <code>exp/&lt;IDENTIFIER&gt;</code> branch, the MCP wiring, the
              agent opening its own pull request, and the issue moving to In
              Review when it does.
            </p>
          </DocsSection>

          {/* ── 04 Run as a daemon ── */}
          <DocsSection id="daemon" num="04" label="Run as a daemon">
            <h2>Run as a daemon</h2>
            <p>
              The daemon turns a machine into a permanent home for agent
              work — a spare workstation, a VPS, the build box under the
              desk:
            </p>
            <DocsCode language="shell">{`
exponential daemon install
`}</DocsCode>
            <p>
              On Linux that writes a <strong>systemd user unit</strong>{` `}
              (<code>exponential-daemon</code>) and enables it; on macOS it
              writes a <strong>launchd agent</strong> that starts at login.
              {` `}
              <code>exponential daemon status</code> tells you whether
              it&apos;s up, and <code>daemon uninstall</code> removes the
              service again. Name the machine with{` `}
              <code>--label</code> if the hostname isn&apos;t what your team
              would recognise.
            </p>
            <DocsCallout kind="note" title="Keep it running after logout">
              A systemd user unit stops when your last session ends unless
              lingering is enabled. Run{` `}
              <code>loginctl enable-linger $USER</code> once on the server.
            </DocsCallout>
            <p>
              The machine then shows up under{` `}
              <strong>Agents → My machines</strong> in the web app, with its
              online state and the agents it has installed. Start a coding
              session or an <a href="/docs/actions/">action</a> there and
              pick that machine, and it runs on it exactly like it would on
              the desktop app: the same launcher, the same live activity
              feed, the same steering from web, iOS, or Android. Batch runs
              and the built-in <em>Fix merge conflicts</em> action work too.
            </p>
            <p>
              Sessions belong to you, not to the box — the agent runs under
              your account, with your own agent subscription and your own
              GitHub access. A machine with no agent CLI installed registers
              as offline until you install one; the daemon notices without a
              restart.
            </p>
          </DocsSection>

          {/* ── 05 Updating ── */}
          <DocsSection id="updating" num="05" label="Updating">
            <h2>Updating</h2>
            <DocsCode language="shell">{`
exponential update
`}</DocsCode>
            <p>
              It checks the latest CLI release, verifies the download against
              the release checksums, and replaces the running binary in
              place. If your instance has stopped supporting the version
              you&apos;re on, every command says so and points here.
            </p>
            <p>
              You rarely need it, though: on its first run the CLI asks
              whether to keep itself up to date automatically. With
              auto-update on, commands check once a day and restart
              themselves on the new build, and the daemon updates on its own
              schedule, waiting until no session is running before it
              restarts. Each machine&apos;s row on the Agents page also shows
              its version with an Update button that asks the daemon to
              update right away — while sessions are still running the row
              reads &quot;Update queued&quot; and the update applies as soon
              as the last one closes. The stored choice is{` `}
              <code>cliAutoUpdate</code> in settings.json.
            </p>
            <p>
              Release notes for the CLI and everything else live on{` `}
              <a href={LINKS.github.releases}>GitHub Releases</a>.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
