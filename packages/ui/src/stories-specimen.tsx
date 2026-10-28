import type { ReactNode } from "react"
import {
  STORIES_COPY,
  STORY_VIEWER,
  type Story,
  type StoryRing as StoryRingModel,
} from "@exp/domain-contract/stories"
import { planCardStandIn } from "./plan-card-specimen"
import { StoryAvatar, StoryRingRow, StoryViewerPanel, storyCardDetail } from "./stories"

// S-3 STORIES: the story ring + viewer specimens (fixtures/stories.json),
// drawn by the styleguide entries `story-ring` / `story-viewer` and the
// app's DEV-ONLY `?dev=story-ring` / `?dev=story-viewer` captures; the
// natives' specimens draw the same rings and story.

const ASSISTANT = { icon: `bot`, color: `#8b5cf6` }
const OPS = { icon: `rocket`, color: `#f59e0b` }

const story = (over: Partial<Story> & Pick<Story, `id` | `kind`>): Story => ({
  sessionId: `sg-${over.id}`,
  rootId: `sg-${over.id}`,
  sessionName: `Fix the nav`,
  waiting: false,
  seen: false,
  createdAt: 0,
  meta: `${STORIES_COPY[over.kind]} · 5m`,
  title: null,
  pictures: [],
  surface: false,
  text: null,
  footer: over.kind === `plan` ? `plan` : over.kind === `question` ? `choices` : `none`,
  autoAdvanceMs: null,
  ...over,
})

/** The four rings of the specimen: the Assistant's (first whatever its
 *  state), a chat with a waiting question, an unseen result, a seen one. */
export const STORY_RING_SPECIMEN: StoryRingModel[] = [
  {
    key: `agent:sg-assistant`,
    agentId: `sg-assistant`,
    rootId: null,
    title: `Assistant`,
    assistant: true,
    waiting: 0,
    unseen: true,
    stories: [
      story({ id: `a1`, kind: `result`, sessionName: `Assistant`, title: `Inbox triaged`, autoAdvanceMs: 6000 }),
      story({ id: `a2`, kind: `result`, sessionName: `Assistant`, seen: true, title: `Standup notes` }),
    ],
  },
  {
    key: `root:sg-nav`,
    agentId: null,
    rootId: `sg-nav`,
    title: `Fix the nav`,
    assistant: false,
    waiting: 1,
    unseen: true,
    stories: [story({ id: `q1`, kind: `question`, waiting: true, title: `Which colour?` })],
  },
  {
    key: `agent:sg-ops`,
    agentId: `sg-ops`,
    rootId: null,
    title: `Ops`,
    assistant: false,
    waiting: 0,
    unseen: true,
    stories: [
      story({ id: `r1`, kind: `result`, sessionName: `Nightly deploy`, title: `Deploy green`, autoAdvanceMs: 6000 }),
      story({ id: `r2`, kind: `result`, sessionName: `Smoke tests`, title: `Smoke tests pass`, autoAdvanceMs: 6000 }),
      story({ id: `r3`, kind: `result`, sessionName: `Smoke tests`, seen: true, title: `Lighthouse 98` }),
    ],
  },
  {
    key: `root:sg-login`,
    agentId: null,
    rootId: `sg-login`,
    title: `Ship the login`,
    assistant: false,
    waiting: 0,
    unseen: false,
    stories: [story({ id: `s1`, kind: `result`, sessionName: `Ship the login`, seen: true, title: `Login shipped` })],
  },
]

/** The specimen rings' avatars (the agent's mark / the session's CLI). */
export function storySpecimenAvatar(ring: Pick<StoryRingModel, `agentId` | `assistant`>): ReactNode {
  if (ring.assistant) return <StoryAvatar agent={ASSISTANT} />
  if (ring.agentId) return <StoryAvatar agent={OPS} />
  return <StoryAvatar cli="claude" />
}

export function StoryRingSpecimen() {
  return (
    <div
      data-testid="story-ring-specimen"
      className="flex w-[24rem] max-w-full flex-col gap-3 rounded-3xl border border-glass-stroke-card bg-background p-4"
    >
      <StoryRingRow rings={STORY_RING_SPECIMEN} avatarFor={storySpecimenAvatar} onOpen={() => {}} />
    </div>
  )
}

/** The viewer's specimen card: a draft question over an eUI surface with
 *  two choices. */
export const STORY_VIEWER_SPECIMEN_CARD = {
  kind: `question`,
  story: true,
  question: `Which header ships?`,
  options: [`Compact`, `Spacious`],
  custom: false,
  surface: {
    root: {
      id: `s`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm` },
      children: [
        { id: `t`, component: `Text`, props: { text: `Two header variants for the board page` } },
        {
          id: `c`,
          component: `ChoiceTiles`,
          props: {
            name: `variant`,
            options: [
              { label: `Compact`, value: `compact` },
              { label: `Spacious`, value: `spacious` },
            ],
            value: { path: `/variant` },
          },
        },
      ],
    },
    data: { variant: `compact` },
  },
} as const

export const STORY_VIEWER_SPECIMEN_RING: StoryRingModel = {
  key: `root:sg-header`,
  agentId: null,
  rootId: `sg-header`,
  title: `Redesign the header`,
  assistant: false,
  waiting: 1,
  unseen: true,
  stories: [
    story({
      id: `sv-q`,
      kind: `question`,
      sessionName: `Redesign the header`,
      waiting: true,
      title: STORY_VIEWER_SPECIMEN_CARD.question,
      surface: true,
      meta: `${STORIES_COPY.question} · 2m`,
    }),
    story({
      id: `sv-r`,
      kind: `result`,
      sessionName: `Redesign the header`,
      title: `Header shipped`,
      pictures: [{ label: `header`, attachmentId: `att-2`, width: 800, height: 500, caption: null }],
      autoAdvanceMs: 6000,
    }),
  ],
}

/** THE viewer frame, open on its first story (the modal's size, drawn in
 *  place so a capture needs no portal). */
export function StoryViewerSpecimen({ frame = true }: { frame?: boolean }) {
  const ring = STORY_VIEWER_SPECIMEN_RING
  const first = ring.stories[0]!
  return (
    <div
      data-testid="story-viewer-specimen"
      className={frame ? `overflow-hidden border border-glass-stroke-card shadow-2xl shadow-black/40` : undefined}
      style={{
        width: STORY_VIEWER.width,
        maxWidth: `100%`,
        height: Math.min(STORY_VIEWER.height, 680),
        borderRadius: frame ? STORY_VIEWER.radius : undefined,
      }}
    >
      <StoryViewerPanel
        ring={ring}
        story={first}
        index={0}
        fraction={1}
        avatar={<StoryAvatar cli="claude" />}
        detail={storyCardDetail(STORY_VIEWER_SPECIMEN_CARD)}
        attachmentSrc={planCardStandIn}
        onPrev={() => {}}
        onNext={() => {}}
        onClose={() => {}}
        onOpenSession={() => {}}
        onAnswer={() => {}}
      />
    </div>
  )
}
