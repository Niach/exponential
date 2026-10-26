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
  { id: `remote`, num: `04`, label: `Other devices` },
  { id: `daemon`, num: `05`, label: `Run as a daemon` },
  { id: `updating`, num: `06`, label: `Updating` },
]

/* Command reference — mirroring the binary's own usage text
   (apps/desktop/crates/cli/src/main.rs). */
const COMMANDS: { name: string; desc: string }[] = [
  { name: `exponential login [--instance <url>] [--install-token <t>] [--no-browser]`, desc: `Sign in to an instance and store the session locally. --install-token redeems a one-time token from the web's Add device dialog; --no-browser only prints the device code.` },
  { name: `exponential logout`, desc: `Sign out and drop the local credentials.` },
  { name: `exponential whoami`, desc: `Show the signed-in account and its instance.` },
  { name: `exponential status`, desc: `Account, this machine's device row, daemon state, auto-update, installed agents, and git in one summary. With no daemon running it says so plainly: the machine is not a device yet.` },
  { name: `exponential doctor`, desc: `The device readiness block every client shows for this machine: Git under Required, Claude Code and Codex under Coding agents, Computer use, each with what to do about it. Exits non-zero only when nothing can run a coding session here.` },
  { name: `exponential code <ISSUE> [--device <label|id>] [--agent claude|codex] [--model <m>] [--effort <e>] [--plan] [--detach] [--account <profile>] [--follow]`, desc: `Start a coding session for an issue, by identifier ("EXP-42") or id. --device starts it on another of your devices instead.` },
  { name: `exponential run <prompt> [--device <label|id>] [--team <id>] [--prompt <text>] [--agent <a>] [--model <m>] [--effort <e>] [--plan] [--detach] [--account <profile>] [--follow]`, desc: `Run one of the team's saved prompts by name or id, as a chat. --prompt adds instructions after its body. The same agent flags apply.` },
  { name: `exponential devices [--team <id>]`, desc: `Your devices (desktop apps and daemons), online or not, with their agents and running sessions. --team adds the servers teammates shared with that team.` },
  { name: `exponential sessions [--device <d>] [--status queued|running|idle|ended] [--agents] [--limit n]`, desc: `The chats you can see, newest first; --agents lists the team agents instead.` },
  { name: `exponential sessions show | log [--follow] | message | stop | end <id>`, desc: `One session's status, device, branch and issues; its transcript; a message to it (an idle session wakes up; - reads the text from stdin); stop interrupts its current turn and leaves it idle; end ends it and its sub-sessions. An id prefix is enough.` },
  { name: `exponential mcp list | connect <server> | set-secret <server> | disconnect <server> [--team <id>]`, desc: `Your teams' MCP servers and your connection to each. The server holds your credential, so one connection works on every device, remote start and trigger: connect opens the server's connect page in your signed-in browser, set-secret reads a key from a no-echo prompt (never argv), disconnect deletes your credential.` },
  { name: `exponential daemon [--foreground] [--label <name>]`, desc: `Run the device daemon in this terminal: it registers the machine and runs the sessions queued for it.` },
  { name: `exponential daemon install | uninstall | status`, desc: `Manage the systemd user unit (Linux) or launchd agent (macOS).` },
  { name: `exponential update [--auto on|off]`, desc: `Self-update from the latest CLI release, or switch automatic updates on or off.` },
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
              Start coding sessions and run prompts from any terminal — and
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
              verifies the binary&apos;s checksum, installs it to{` `}
              <code>~/.local/bin/exponential</code>, and then sets the machine
              up in one go: it turns on automatic updates, signs you in,
              installs the <a href="#daemon">daemon</a> as a service that
              starts at boot, and ends with a summary of the account, the
              device name, the daemon and auto-update. Without a terminal
              (cloud-init, a provisioning script) it still signs in: it
              prints the device code and waits until you approve it in a
              browser.
            </p>
            <p>
              The web app&apos;s <strong>Add device</strong> dialog gives you
              the same command with a one-time install token in it, which
              signs in with no approval step:
            </p>
            <DocsCode language="shell">{`
curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://issues.example.com EXP_INSTALL_TOKEN=expi_... sh
`}</DocsCode>
            <p>
              Opt out of any step with <code>EXP_NO_DAEMON=1</code> or{` `}
              <code>EXP_NO_AUTOUPDATE=1</code>, sign in with an existing API
              key through <code>EXP_TOKEN</code>, and name the machine with{` `}
              <code>EXP_DEVICE_LABEL</code>.
            </p>
            <p>
              You also need <code>git</code>, plus at least one coding
              agent — <strong>Claude Code</strong> (its CLI on your{` `}
              <code>PATH</code>) or <strong>Codex</strong>, which needs no
              install: the machine downloads its own pinned Codex build the
              first time you sign in to it. See{` `}
              <a href="/docs/coding/">Coding agents</a> for what each one
              supports. The installer ends by printing the machine&apos;s
              readiness block, the same one <code>exponential doctor</code>{` `}
              prints whenever you run it again.
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
              under <strong>Settings → Security</strong> in the web app and
              hand it to the CLI as <code>EXP_TOKEN</code>:
            </p>
            <DocsCode language="shell">{`
EXP_INSTANCE=https://issues.example.com EXP_TOKEN=expu_... exponential login
`}</DocsCode>
            <p>
              Or redeem an install token from the <strong>Add device</strong>{` `}
              dialog. It works once and expires after 15 minutes:
            </p>
            <DocsCode language="shell">{`
exponential login --install-token expi_...
`}</DocsCode>
            <p>
              Signing in does not make the machine a device. Only the daemon
              does that: until it runs, the machine is not listed on your
              other clients and nothing can be started on it remotely.{` `}
              <code>exponential status</code> tells you which it is.
            </p>
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
              options as the Agent page composer, and fall back to your saved
              per-agent defaults when you omit them. <code>--plan</code> works
              on both agents; every session bypasses the agent&apos;s permission
              prompts. See{` `}
              <a href="/docs/coding/">Coding agents</a>.
            </p>
            <p>
              Run <code>code</code> with a terminal attached and the session
              prints there as a line transcript — the same narration, tool
              calls and questions the app shows — and whatever you type is
              sent to the agent as a message. Without a terminal (in CI, over
              a pipe) or with <code>--detach</code>, the session runs headless
              and stays fully steerable from the web, so you can close the
              laptop and keep watching it from your phone.
            </p>
            <DocsCode language="shell">{`
exponential code EXP-42 --agent claude --plan
exponential run "Release train" --prompt "version 1.4.0"
`}</DocsCode>
            <p>
              Either way it&apos;s a real{` `}
              <a href="/docs/coding/">coding session</a>: a worktree on an{` `}
              <code>exp/&lt;IDENTIFIER&gt;</code> branch, the MCP wiring, the
              agent opening its own pull request, and the issue moving to In
              Review when it does.
            </p>
          </DocsSection>

          {/* ── 04 Other devices ── */}
          <DocsSection id="remote" num="04" label="Other devices">
            <h2>Other devices</h2>
            <p>
              The CLI can drive every device you own, not just the one
              it&apos;s on. <code>exponential devices</code> lists them with
              their online state, agents and running sessions. Add{` `}
              <code>--device</code> to <code>code</code> or <code>run</code>{` `}
              and the session starts on that machine, the way the Agent page
              starts it. Agent options you leave out come from that
              machine&apos;s own launch defaults:
            </p>
            <DocsCode language="shell">{`
exponential devices
exponential code EXP-42 --device "build box" --agent claude --follow
exponential run "Fix flaky tests" --device "build box" --prompt "Start with the e2e suite"
exponential sessions --status running
exponential sessions log 1a2b3c4d --follow
exponential sessions message 1a2b3c4d "Use the staging database instead"
exponential sessions stop 1a2b3c4d
`}</DocsCode>
            <p>
              A device is picked by its name or id. A start aimed at an
              offline device is refused. <code>--account</code> picks an agent
              account on the target, and <code>--follow</code> tails the
              session&apos;s transcript right after the start. Transcripts live only on the device that
              ran the session, so <code>sessions log</code> streams them
              through the steer relay like the app does. That needs the
              device online, and a live session shows its transcript only to
              its owner.
            </p>
          </DocsSection>

          {/* ── 05 Run as a daemon ── */}
          <DocsSection id="daemon" num="05" label="Run as a daemon">
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
              A systemd user unit stops when your last session ends and
              never starts at boot unless lingering is enabled.{` `}
              <code>daemon install</code> turns it on when your account is
              allowed to. When it isn&apos;t, it prints the one command to
              run:{` `}
              <code>sudo loginctl enable-linger $USER</code>.
            </DocsCallout>
            <p>
              The machine then shows up under{` `}
              <strong>Devices → My devices</strong> in the web app, with its
              online state and the agent logins it holds. Pick it in the
              Agent page composer&apos;s <strong>Device</strong> picker,
              start a coding session or a{` `}
              <a href="/docs/actions/">prompt</a>, and it runs on that machine
              exactly like it would on the desktop app: the same launcher,
              the same live transcript, the same steering from web, iOS, or
              Android. Chats over several issues, free requests and the
              conversations of{` `}
              <a href="/docs/actions/#triggers">team agents</a> set to run on
              it run there too. When the daemon restarts, it claims its
              unfinished sessions back and carries on.
            </p>
            <p>
              A session you start on the daemon makes no report, exactly like a
              session in the desktop app or an attached CLI: when the agent
              finishes its turn it waits for your next reply, and the daemon
              keeps its worktree with no idle timeout. The composer&apos;s
              stop button in the web or mobile app (or{` `}
              <code>exponential sessions stop</code>) interrupts the current
              turn and leaves the session waiting; <strong>Stop</strong> in
              the chat&apos;s header or{` `}
              <code>exponential sessions end</code> ends it. Sessions another
              session started (a chat&apos;s child sessions, a chat a{` `}
              <a href="/docs/actions/#triggers">team agent</a> starts) are the
              ones that end themselves when their work is done, and a
              team agent&apos;s own conversation never ends.
            </p>
            <p>
              Sessions belong to you, not to the box — the agent runs under
              your account, with your own agent subscription and your own
              GitHub access. A machine with no coding agent ready registers
              as offline until it has one; the daemon notices without a
              restart. A start aimed at a machine that is offline is refused
              on the spot rather than queued.
            </p>
            <h3>Running it from a phone</h3>
            <p>
              Everything you would set at the desk is on the machine&apos;s
              row on the <strong>Devices</strong> page (on a phone, the last
              row of the Team tab) and in its{` `}
              <strong>Device settings</strong> (the row&apos;s gear), from any
              client:
            </p>
            <ul>
              <li>
                Its <strong>Name</strong>, whether it is your{` `}
                <strong>Default device</strong> (preselected in every picker,
                marked with a star in the list),
                and under <strong>Sharing</strong> which teams it is shared
                with: teammates can then start sessions on it. Withdrawing the
                share ends their running sessions.
              </li>
              <li>
                Its readiness: the same rows <code>exponential doctor</code>{` `}
                prints, with <strong>Sign in</strong> and{` `}
                <strong>Update</strong> for each coding agent and the{` `}
                <strong>Computer use</strong> switch (plus its{` `}
                <strong>Computer use model</strong> while it is on).
              </li>
              <li>
                Its launch defaults, one tab per agent: <strong>Model</strong>,{` `}
                <strong>Chat model</strong>, <strong>Subagent model</strong>,{` `}
                <strong>Effort</strong>, <strong>Ultracode</strong>,{` `}
                <strong>Plan mode</strong> and{` `}
                <strong>Auto-rotate accounts</strong>.
              </li>
              <li>
                The agent logins it holds, listed under its row with their
                rate-limit usage. <strong>Sign in</strong> and{` `}
                <strong>Add account</strong> run the agent&apos;s own sign-in
                on the machine and hand you back the code and link as an
                answerable card, so a headless box never needs a browser on
                it.
              </li>
              <li>
                <strong>Update</strong> for the daemon itself, and{` `}
                <strong>Remove device</strong>.
              </li>
            </ul>
          </DocsSection>

          {/* ── 06 Updating ── */}
          <DocsSection id="updating" num="06" label="Updating">
            <h2>Updating</h2>
            <DocsCode language="shell">{`
exponential update
`}</DocsCode>
            <p>
              It checks the latest CLI release, verifies the download against
              the release checksums, and replaces the running binary in
              place. It also restarts an idle service-managed daemon onto the
              new build. If your instance has stopped supporting the version
              you&apos;re on, every command says so and points here — and a
              daemon that gets locked out this way updates itself right away
              rather than waiting for its next scheduled check.
            </p>
            <p>
              You rarely need it, though: the install script turns automatic
              updates on (<code>EXP_NO_AUTOUPDATE=1</code> leaves them off),
              and a CLI installed some other way asks on its first session.{` `}
              <code>exponential update --auto on</code> or{` `}
              <code>--auto off</code> changes it later. With
              auto-update on, commands check once a day and restart
              themselves on the new build, and the daemon updates on its own
              schedule, waiting until no session is running before it
              restarts. A server&apos;s <strong>Device settings</strong> also
              show its version under <strong>Update</strong>, with an{` `}
              <strong>Update</strong> button that asks the daemon to update
              right away. While sessions are still running it reads{` `}
              <strong>Queued</strong> and the update applies as soon as the
              last one closes; <strong>Update now…</strong> ends them and
              restarts on the new version at once. The stored choice is{` `}
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
