import type { ReactNode } from "react"
import {
  stories,
  STORY_VIEWER,
  type StoriesInput,
  type StoryAgentInput,
  type StoryRing as StoryRingModel,
} from "@exp/domain-contract/stories"
import { StoryAvatar, StoryProgress, StoryRing, StoryRingRow, StoryViewerPanel, storyCardDetail } from "./stories"

// S-3 STORIES: THE styleguide specimens (fixtures/stories.json `specimen`,
// its `$comment` is the spec), drawn by the styleguide entries `story-ring` /
// `story-viewer` and the app's DEV-ONLY `?dev=story-ring` / `?dev=story-viewer`
// captures. The input is hardcoded identically on every platform (desktop
// styleguide entries, iOS, Android StorySpecimens) and `cases` replays it.

/** An agent of the specimen: the fold's input plus its mark (the bubble's
 *  icon + colour). */
type SpecimenAgent = StoryAgentInput & { icon: string; color: string }

/** stories.json `specimen.input`. */
export const STORY_SPECIMEN_INPUT: Omit<StoriesInput, `agents`> & { agents: readonly SpecimenAgent[] } = {
  now: 1800000000000,
  sessions: [
    {
      id: `triage`,
      rootId: `triage`,
      name: `Triage`,
      agentId: `a-triage`,
      startedByAgentId: null,
    },
    {
      id: `board`,
      rootId: `board`,
      name: `Board header polish`,
      agentId: null,
      startedByAgentId: null,
    },
    {
      id: `designer`,
      rootId: `designer`,
      name: `Designer`,
      agentId: `a-designer`,
      startedByAgentId: null,
    },
    {
      id: `ship`,
      rootId: `ship`,
      name: `Ship the launch checklist`,
      agentId: null,
      startedByAgentId: null,
    },
  ],
  agents: [
    {
      id: `a-triage`,
      name: `Triage`,
      assistant: false,
      icon: `bug`,
      color: `#a855f7`,
    },
    {
      id: `a-designer`,
      name: `Designer`,
      assistant: false,
      icon: `palette`,
      color: `#ec4899`,
    },
  ],
  messages: [
    {
      id: `triage-plan`,
      sessionId: `triage`,
      authorSessionId: `triage`,
      createdAt: 1799999460000,
      answeredAt: null,
      card: {
        kind: `plan`,
        plan: `## Close the duplicates\n\nMerge FB-8 into FB-3 and label the rest.`,
        decision: null,
      },
    },
    {
      id: `triage-result`,
      sessionId: `triage`,
      authorSessionId: `triage`,
      createdAt: 1799998200000,
      answeredAt: null,
      card: {
        kind: `surface`,
        story: true,
        title: `Today's triage`,
        surface: {
          root: {
            id: `r`,
            component: `Text`,
            props: {
              text: `3 labelled`,
            },
          },
        },
      },
    },
    {
      id: `board-question`,
      sessionId: `board`,
      authorSessionId: `board`,
      createdAt: 1799999760000,
      answeredAt: null,
      card: {
        kind: `question`,
        question: `Which accent for the board header?`,
        options: [`Indigo`, `Emerald`],
        optionDescriptions: [`The brand accent`, `Matches the merged state`],
        custom: false,
        surface: {
          root: {
            id: `root`,
            component: `Stack`,
            props: {
              gap: `md`,
            },
            children: [
              {
                id: `title`,
                component: `Text`,
                props: {
                  text: `Board header`,
                },
                children: [],
              },
              {
                id: `accent`,
                component: `ChoiceTiles`,
                props: {
                  name: `accent`,
                  options: [
                    {
                      label: `Indigo`,
                      value: `indigo`,
                    },
                    {
                      label: `Emerald`,
                      value: `emerald`,
                    },
                  ],
                  value: {
                    path: `/accent`,
                  },
                },
                children: [],
              },
              {
                id: `stat`,
                component: `StatTile`,
                props: {
                  label: `Header height`,
                  value: `48 px`,
                  delta: `−8 px`,
                  tone: `success`,
                },
                children: [],
              },
            ],
          },
          data: {
            accent: `indigo`,
          },
        },
      },
    },
    {
      id: `designer-result`,
      sessionId: `designer`,
      authorSessionId: `designer`,
      createdAt: 1799996400000,
      answeredAt: null,
      card: {
        kind: `surface`,
        story: true,
        title: `Spacing review`,
        surface: {
          root: {
            id: `r`,
            component: `Text`,
            props: {
              text: `No drift`,
            },
          },
        },
      },
    },
    {
      id: `ship-result`,
      sessionId: `ship`,
      authorSessionId: `ship`,
      createdAt: 1799992800000,
      answeredAt: null,
      card: {
        kind: `surface`,
        story: true,
        title: `Checklist done`,
        surface: {
          root: {
            id: `r`,
            component: `Text`,
            props: {
              text: `12 of 12`,
            },
          },
        },
      },
    },
  ],
  reads: [
    {
      sessionId: `ship`,
      readAt: 1799996400000,
      storySeenAt: null,
    },
  ],
}

/** stories.json `specimen.smallRing`: the rings again, without titles. */
export const STORY_SPECIMEN_SMALL_RING = 36

/** stories.json `specimen.progress`. */
export const STORY_SPECIMEN_PROGRESS = { count: 4, index: 1, fraction: 0.4 } as const

/** The specimen's rings = the fold of its input (Board header polish,
 *  Triage, Designer, Ship the launch checklist). */
export const STORY_RING_SPECIMEN: StoryRingModel[] = stories(STORY_SPECIMEN_INPUT)

/** A ring's avatar: an agent ring = its icon in its colour on the muted disc
 *  (the bubble's look); a chat ring = the chat row's BARE brand mark (the
 *  specimen sessions carry no CLI = the default mark). */
export function storySpecimenAvatar(ring: Pick<StoryRingModel, `agentId`>): ReactNode {
  const agent = ring.agentId ? STORY_SPECIMEN_INPUT.agents.find((row) => row.id === ring.agentId) : undefined
  if (agent) return <StoryAvatar agent={{ icon: agent.icon, color: agent.color }} />
  return <StoryAvatar cli={null} />
}

export function StoryRingSpecimen() {
  return (
    <div
      data-testid="story-ring-specimen"
      className="flex w-[24rem] max-w-full flex-col gap-4 rounded-3xl border border-glass-stroke-card bg-background p-4"
    >
      <StoryRingRow rings={STORY_RING_SPECIMEN} avatarFor={storySpecimenAvatar} onOpen={() => {}} />
      <div data-slot="story-ring-specimen-small" className="flex gap-3">
        {STORY_RING_SPECIMEN.map((ring) => (
          <StoryRing key={ring.key} stories={ring.stories} size={STORY_SPECIMEN_SMALL_RING}>
            {storySpecimenAvatar(ring)}
          </StoryRing>
        ))}
      </div>
      <StoryProgress
        count={STORY_SPECIMEN_PROGRESS.count}
        index={STORY_SPECIMEN_PROGRESS.index}
        fraction={STORY_SPECIMEN_PROGRESS.fraction}
      />
    </div>
  )
}

/** The viewer's ring (`specimen.viewer.ring` = 0) and its first story. */
export const STORY_VIEWER_SPECIMEN_RING: StoryRingModel = STORY_RING_SPECIMEN[0]!

/** The raw card of the viewer's story (the waiting board question). */
export const STORY_VIEWER_SPECIMEN_CARD = STORY_SPECIMEN_INPUT.messages.find(
  (message) => message.id === STORY_VIEWER_SPECIMEN_RING.stories[0]!.id
)!.card

/** THE viewer frame, open on ring 0, story 0, the progress frozen at 0 (the
 *  modal's size, drawn in place so a capture needs no portal). */
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
        fraction={0}
        avatar={storySpecimenAvatar(ring)}
        detail={storyCardDetail(STORY_VIEWER_SPECIMEN_CARD)}
        onPrev={() => {}}
        onNext={() => {}}
        onClose={() => {}}
        onOpenSession={() => {}}
        onAnswer={() => {}}
      />
    </div>
  )
}
