// EXP-820: the chips over the agent page's empty prompt box. A POOL rather
// than three fixed chips — the chat is a conversation with an agent that holds
// the whole product's MCP surface (issues, boards, labels, automations,
// sessions on other devices, reviews), so the chips are there to SHOW that
// range, the way the getting-started cards do: each mount draws a few at
// random. A suggestion ending in `#` lands the caret after it so the issue-ref
// autocomplete opens at once (EXP-790); the rest are complete prompts.
// Hand-mirrored byte-for-byte on the desktop (`chat_screen.rs`
// `CHAT_SUGGESTIONS`) — change both together.

export const CHAT_SUGGESTION_POOL: readonly string[] = [
  `Fix #`,
  `Explain #`,
  `Review #`,
  `Split # into sub-issues`,
  `Label every issue in the backlog`,
  `Set a priority on every unprioritized issue`,
  `Find duplicate issues and link them`,
  `Do a code review of the open PRs and file the findings on a new board`,
  `Create an automation that labels new issues`,
  `Set up a weekly standup digest automation`,
  `Draft release notes from the issues completed this month`,
  `Summarize what changed across the boards this week`,
  `Start a session for # on my other machine`,
  `Move stale in-progress issues back to the backlog`,
  `Comment a plan on #`,
  `Which issues are blocked, and by what?`,
]

/** How many chips a mount shows — enough to read as a range, few enough
 *  to stay one row on a laptop. */
export const CHAT_SUGGESTION_COUNT = 4

/** `count` distinct suggestions drawn from the pool, in pool order, using
 *  `random` (`Math.random` by default; a seeded function in tests). */
export function pickChatSuggestions(
  count = CHAT_SUGGESTION_COUNT,
  random: () => number = Math.random
): string[] {
  const indices = CHAT_SUGGESTION_POOL.map((_, i) => i)
  // A partial Fisher–Yates: the first `count` slots end up a uniform draw.
  const take = Math.min(count, indices.length)
  for (let i = 0; i < take; i++) {
    const j = i + Math.floor(random() * (indices.length - i))
    ;[indices[i], indices[j]] = [indices[j], indices[i]]
  }
  return indices
    .slice(0, take)
    .sort((a, b) => a - b)
    .map((i) => CHAT_SUGGESTION_POOL[i])
}
