/* The home-page FAQ (MKT-9, also MKT-4). ONE array feeds the rendered
   <HomeFaq/> and the FAQPage JSON-LD in seo.ts, so the structured data can
   never claim something the page doesn't say. Answers are plain text (no
   markup): JSON-LD wants text, and every claim here has to hold. */

export type FaqItem = { q: string; a: string }

export const HOME_FAQ: FaqItem[] = [
  {
    q: `What is Exponential?`,
    a: `An open-source issue tracker built for teams that code with agents. Boards, issues, relations, reviews and a support inbox sync in realtime across web, desktop, iOS and Android. Every coding run you start from it gets Exponential as its MCP server, so the agent reads its issue, files follow-ups, opens and merges its pull request and asks you when it needs a decision.`,
  },
  {
    q: `Which coding agents does it work with?`,
    a: `Claude Code and Codex. Exponential runs your own, unmodified CLI with your own login, so your existing subscription pays for the tokens. You can add several accounts per machine, and Claude runs can start on the account with the most headroom.`,
  },
  {
    q: `Where do the agent runs happen?`,
    a: `On your machines. Runs start in the desktop app on macOS, Windows or Linux, or in the headless exponential CLI daemon on a server you own. There are no cloud agents: you start and steer runs from your phone or the web, and the work happens on hardware you control.`,
  },
  {
    q: `Is it open source? Can I self-host it?`,
    a: `Yes. Exponential is Apache-2.0 licensed. Self-hosting is one Docker Compose file that pulls the published image, with no plan limits. The one thing self-hosted instances miss is push notifications on the store mobile apps.`,
  },
  {
    q: `How much does it cost?`,
    a: `The cloud is free for teams of up to three. The Team plan is billed per seat and covers more seats, storage and the helpdesk. Agent runs are never plan-gated, and self-hosting is free at any size.`,
  },
  {
    q: `How does it compare to Linear?`,
    a: `Linear is a mature, polished tracker for teams. Exponential is younger and built around one idea: your coding agents work inside the tracker. The runs happen on your own machines with your own agent subscription, and each one can use the tracker itself: file issues, start further runs, open and merge pull requests and publish screenshots. It is also open source and self-hostable.`,
  },
]
