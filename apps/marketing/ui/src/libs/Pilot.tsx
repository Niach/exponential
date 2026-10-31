/* EXP-1421: exponential.at/pilot/, ePilot's one docs page. Facts:
   apps/desktop/crates/epilot (EXP-1273, the tools and the lock) and
   apps/desktop/crates/epilot-screen (screen memory,
   fixtures/screen-memory.json), both renamed and made consumable as a git
   dependency by EXP-1425; the names are docs/libraries.md's. Tag
   epilot-v0.1.0; since EXP-1492 (#1499) master serves the tools through
   emcp, which the next tag carries. */
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, IcArrow, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `loop`, num: `02`, label: `Plan, act, observe` },
  { id: `tools`, num: `03`, label: `The tools` },
  { id: `foreground`, num: `04`, label: `Foreground only` },
  { id: `memory`, num: `05`, label: `Screen memory` },
  { id: `platforms`, num: `06`, label: `Platforms` },
  { id: `packages`, num: `07`, label: `Packages` },
]

const TOOLS: [string, string][] = [
  [`screenshot`, `The current display as a PNG fitted to at most 1280 × 800. Every other tool's coordinates are in this image's pixels.`],
  [`list_windows`, `Every top-level window on the work screen, top first, plus the displays.`],
  [`activate_window`, `Bring a window to the front and give it the keyboard.`],
  [`cursor_position`, `Where the pointer is, in screenshot pixels.`],
  [`mouse_move`, `Move the pointer (a hover).`],
  [`left_click`, `Also right_click, middle_click and double_click: move there and click.`],
  [`drag`, `Press at one point, move to another, release.`],
  [`scroll`, `Move there and turn the wheel.`],
  [`type`, `Type up to 4,000 characters into the focused control.`],
  [`key`, `One key or combo: Return, ctrl+s, alt+Tab, cmd+space.`],
  [`wait`, `Let the screen settle (up to 10 s), then take a screenshot.`],
]

export default function Pilot({ path }: PageProps) {
  return (
    <>
      <PageHero
        eyebrow="Library"
        title="ePilot"
        cta={
          <a className="btn btn-ghost" href={LINKS.source(`apps/desktop/crates/epilot`)}>
            Source <IcArrow />
          </a>
        }
      >
        Computer use for agents: the agent looks at the real screen, acts on it with the real pointer and keyboard, and looks again.
      </PageHero>
      <DocsLayout nav={LIBRARIES_NAV} title="Libraries" sections={SECTIONS} currentPath={path}>
        <DocsSection id="status" num="01" label="Status">
          <h2>Status</h2>
          <p>
            ePilot runs today inside the Exponential desktop app and the <code>exponential</code> CLI daemon, where coding
            sessions use it. The library is its own pair of crates, consumed as a git dependency on the{` `}
            <code>epilot-v0.1.0</code> tag. On master its tools are served through{` `}
            <a className="content-link" href="/mcp/">eMCP</a>; the next tag carries that.
          </p>
          <DocsTable
            head={[`Part`, `Where`, ``]}
            rows={[
              [<strong key="p">Computer use</strong>, `Coding sessions on the desktop app and the CLI daemon`, <StatusChip key="s" status="in-progress">in the app</StatusChip>],
              [<strong key="p">Screen memory</strong>, `The voice device, for your Assistant`, <StatusChip key="s" status="in-progress">in the app</StatusChip>],
              [<strong key="p">The library</strong>, <span key="w"><code>epilot</code> and <code>epilot-screen</code>, a git dependency</span>, <StatusChip key="s" status="shipped">0.1.0</StatusChip>],
            ]}
          />
        </DocsSection>
        <DocsSection id="loop" num="02" label="Plan, act, observe">
          <h2>Plan, act, observe</h2>
          <p>
            The agent&apos;s model plans; ePilot is its eyes and hands. Each step is a screenshot, one action, and another
            screenshot. Pixels only: no accessibility tree, no DOM, so it drives any app the person can.
          </p>
          <ol>
            <li>
              <strong>Observe</strong>: <code>screenshot</code> fits the display to the model&apos;s frame. Every coordinate
              after that is in that image&apos;s pixels, and ePilot maps it back to the screen.
            </li>
            <li>
              <strong>Plan</strong>: the model reads the image and picks the next action.
            </li>
            <li>
              <strong>Act</strong>: a click, a drag, typed text or a key combo, through the OS&apos;s own input APIs.
            </li>
            <li>
              <strong>Observe again</strong>: <code>wait</code> lets the screen settle and returns the next screenshot.
            </li>
          </ol>
          <p>
            The host serves it as one loopback MCP server, <code>computer</code>. A session gets its URL and a token only
            while the device&apos;s Computer use switch is on.
          </p>
        </DocsSection>
        <DocsSection id="tools" num="03" label="The tools">
          <h2>The tools</h2>
          <DocsTable head={[`Tool`, `What`]} rows={TOOLS.map(([name, what]) => [<code key="n">{name}</code>, what])} />
        </DocsSection>
        <DocsSection id="foreground" num="04" label="Foreground only">
          <h2>Foreground only</h2>
          <p>
            The agent drives the person&apos;s real pointer, so the person&apos;s own mouse and keyboard are held while it
            works. Esc takes them back and stops the agent. The desktop app shows a banner the whole time.
          </p>
          <p>
            The work screen decides what the agent sees and drives: every display without a fullscreen app, or one display
            you pick.
          </p>
        </DocsSection>
        <DocsSection id="memory" num="05" label="Screen memory">
          <h2>Screen memory</h2>
          <p>
            The second half of ePilot keeps the last 10 minutes of the screen as deduplicated frames, on the device only. Its
            one tool, <code>get_screenshots</code>, returns up to 4 frames from a moment, each cut to the window the person
            was in, so a voice agent can see what you are talking about. Off by default; turning it off deletes the buffer.
          </p>
        </DocsSection>
        <DocsSection id="platforms" num="06" label="Platforms">
          <h2>Platforms</h2>
          <DocsTable
            head={[`Platform`, `Input`, `Needs`]}
            rows={[
              [`macOS`, `CGEvent`, `Screen Recording and Accessibility`],
              [`Windows`, `SendInput`, `nothing`],
              [`Linux (X11)`, `XTest`, `an X11 session; Wayland is not supported`],
            ]}
          />
        </DocsSection>
        <DocsSection id="packages" num="07" label="Packages">
          <h2>Packages</h2>
          <p>
            Two crates in the Exponential repo, taken as a git dependency on an <code>epilot-v&lt;version&gt;</code> tag, never
            from crates.io: the rules are compiled in from the repo&apos;s fixtures, which only a checkout carries. Desktop
            only, so there are no Swift, Kotlin or npm packages.
          </p>
          <DocsTable
            head={[`Crate`, `What`]}
            rows={[
              [<code key="n">epilot</code>, `The tools, the input lock and the MCP endpoint`],
              [<code key="n">epilot-screen</code>, `Screen memory and get_screenshots`],
            ]}
          />
          <DocsCode language="toml">{`[dependencies]
epilot = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.1.0" }
epilot-screen = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.1.0" }`}</DocsCode>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
