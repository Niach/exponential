import type { EntryKey } from "@/components/getting-started/getting-started-model"

// EXP-698 r5 — the getting-started checklist runs on all four clients now, so
// its copy is ONE table per platform and `getting-started-copy.test.ts` reads
// the native ones off disk to prove they still say the same thing:
//
//   desktop  crates/ui/src/getting_started.rs      (all ten entries)
//   iOS      UI/GettingStarted/GettingStartedCopy.swift   (the seven mobile ones)
//   Android  ui/gettingstarted/GettingStartedCopy.kt      (the seven mobile ones)
//
// The three web-only entries (widget, helpdesk, mcp) are managed from a
// browser, so the phones never list them. Keep every string free of quotes,
// backslashes and non-ASCII punctuation: the drift test matches them as
// literals inside Swift/Kotlin/Rust source.
//
// `action` is the label of the entry's ONE call to action; the MCP entry has
// no button at all (it renders the per-client setup tabs instead) and carries
// an empty string.
export const GETTING_STARTED_COPY: Record<
  EntryKey,
  { title: string; description: string; action: string }
> = {
  desktop: {
    title: `Get the desktop app`,
    description: `Runs coding sessions on your machine and registers it as one of your devices.`,
    action: `Download the desktop app`,
  },
  github: {
    title: `Connect a GitHub repo`,
    description: `Boards attach repositories; pull requests and coding sessions flow back into issues.`,
    action: `Connect GitHub`,
  },
  invite: {
    title: `Invite your team`,
    description: `Teammates share boards, reviews, and the support inbox.`,
    action: `Invite in team settings`,
  },
  board: {
    title: `Create a board`,
    description: `Boards hold your issues; connect a repository to code on one.`,
    action: `Create a board`,
  },
  coding: {
    title: `Start coding with an agent`,
    description: `Start coding on an issue hands it to your agent, which plans, implements, and opens the PR.`,
    action: `Open Devices`,
  },
  action: {
    title: `Create an action`,
    description: `Reusable agent runs for your team, written by your agent from a description.`,
    action: `New action`,
  },
  server: {
    title: `Set up a server`,
    description: `Run the headless daemon on an always-on machine to take remote Start coding requests.`,
    action: `Copy install command`,
  },
  widget: {
    title: `Set up the feedback widget`,
    description: `Visitors report bugs with an annotated screenshot; each lands here as an issue.`,
    action: `Set up in team settings`,
  },
  helpdesk: {
    title: `Enable the helpdesk`,
    description: `Support tickets from the widget land in a shared Support inbox.`,
    action: `Enable in team settings`,
  },
  mcp: {
    title: `Connect your tools via MCP`,
    description: `Work with issues, boards, and comments from Claude, Cursor, or any MCP client.`,
    action: ``,
  },
}

/** The entries the phones render — the other three are managed on the web. */
export const MOBILE_GETTING_STARTED_KEYS: readonly EntryKey[] = [
  `desktop`,
  `github`,
  `invite`,
  `board`,
  `coding`,
  `action`,
  `server`,
]
