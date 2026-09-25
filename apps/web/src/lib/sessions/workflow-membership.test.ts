import { describe, expect, it } from "vitest"
import {
  matchWorkflowNode,
  resolveWorkflowMembership,
  type WorkflowMembershipNode,
} from "./workflow-membership"

// EXP-1082 §1 — the session membership rule, every branch.

const WF = `wf-running`
const DRAFT = `wf-draft`
const node = (over: Partial<WorkflowMembershipNode> = {}): WorkflowMembershipNode => ({
  workflowId: WF,
  nodeId: `node-1`,
  issueId: `issue-1`,
  memberIssueIds: [],
  workflowStatus: `running`,
  workflowCreatedAt: `2026-09-01T00:00:00Z`,
  ...over,
})
const none = {
  workflowId: null,
  workflowNodeId: null,
  workflowRole: null,
  startedReason: null,
  parentSessionId: null,
}

describe(`resolveWorkflowMembership`, () => {
  it(`gives a plain run no membership`, () => {
    expect(resolveWorkflowMembership({ issueIds: [] })).toEqual(none)
    expect(resolveWorkflowMembership({ issueIds: [`issue-9`], nodes: [node()] })).toEqual(none)
  })

  describe(`(a) explicit host values`, () => {
    it(`win outright`, () => {
      expect(
        resolveWorkflowMembership({
          explicit: { workflowId: WF, workflowNodeId: `node-2`, workflowRole: `review` },
          startedReason: `workflow`,
          issueIds: [`issue-1`],
          nodes: [node()],
        })
      ).toEqual({
        workflowId: WF,
        workflowNodeId: `node-2`,
        workflowRole: `review`,
        startedReason: `workflow`,
        parentSessionId: null,
      })
    })

    it(`beat the predecessor's`, () => {
      expect(
        resolveWorkflowMembership({
          explicit: { workflowId: WF, workflowNodeId: `node-2`, workflowRole: `review` },
          predecessor: {
            workflowId: DRAFT,
            workflowNodeId: `node-9`,
            workflowRole: `author`,
            startedReason: `workflow`,
            parentSessionId: `parent-1`,
          },
          issueIds: [],
        })
      ).toEqual({
        workflowId: WF,
        workflowNodeId: `node-2`,
        workflowRole: `review`,
        startedReason: `workflow`,
        parentSessionId: `parent-1`,
      })
    })

    it(`allow a node-less planner run`, () => {
      expect(
        resolveWorkflowMembership({
          explicit: { workflowId: WF, workflowRole: `plan` },
          issueIds: [],
        })
      ).toMatchObject({ workflowId: WF, workflowNodeId: null, workflowRole: `plan` })
    })
  })

  describe(`(b) a resume`, () => {
    const predecessor = {
      workflowId: WF,
      workflowNodeId: `node-1`,
      workflowRole: `review`,
      startedReason: `workflow`,
      parentSessionId: null,
    }

    it(`inherits all five fields`, () => {
      expect(resolveWorkflowMembership({ predecessor, issueIds: [] })).toEqual(predecessor)
    })

    it(`keeps workflow when a person's agent frame resumes it`, () => {
      expect(
        resolveWorkflowMembership({ predecessor, startedReason: `agent`, issueIds: [] })
      ).toEqual(predecessor)
    })

    it(`keeps the predecessor's parent`, () => {
      expect(
        resolveWorkflowMembership({
          predecessor: { ...predecessor, parentSessionId: `parent-1`, startedReason: `agent` },
          issueIds: [],
        })
      ).toMatchObject({ parentSessionId: `parent-1`, startedReason: `agent` })
    })

    it(`lets the frame fill a person-started predecessor's null reason`, () => {
      expect(
        resolveWorkflowMembership({
          predecessor: { ...none },
          startedReason: `agent`,
          issueIds: [],
        })
      ).toEqual({ ...none, startedReason: `agent` })
    })

    it(`beats a node match on the issue`, () => {
      expect(
        resolveWorkflowMembership({
          predecessor: { ...predecessor, workflowId: DRAFT, workflowNodeId: `node-9` },
          issueIds: [`issue-1`],
          nodes: [node()],
        })
      ).toMatchObject({ workflowId: DRAFT, workflowNodeId: `node-9`, workflowRole: `review` })
    })

    it(`of a run outside any workflow falls through to the node match`, () => {
      expect(
        resolveWorkflowMembership({
          predecessor: { ...none, parentSessionId: `parent-1` },
          issueIds: [`issue-1`],
          nodes: [node()],
        })
      ).toEqual({
        workflowId: WF,
        workflowNodeId: `node-1`,
        workflowRole: `author`,
        startedReason: null,
        parentSessionId: `parent-1`,
      })
    })
  })

  describe(`(c) a child of a workflow run`, () => {
    const parent = { workflowId: WF, workflowNodeId: `node-1` }

    it(`inherits workflow + node, author with an issue subject`, () => {
      expect(
        resolveWorkflowMembership({ parent, startedReason: `agent`, issueIds: [`issue-7`] })
      ).toEqual({
        workflowId: WF,
        workflowNodeId: `node-1`,
        workflowRole: `author`,
        startedReason: `agent`,
        parentSessionId: null,
      })
    })

    it(`gets no role when it is not an issue run`, () => {
      expect(
        resolveWorkflowMembership({ parent, startedReason: `agent`, issueIds: [] })
      ).toMatchObject({ workflowId: WF, workflowNodeId: `node-1`, workflowRole: null })
    })

    it(`of a run outside any workflow falls through to the node match`, () => {
      expect(
        resolveWorkflowMembership({
          parent: { workflowId: null, workflowNodeId: null },
          startedReason: `agent`,
          issueIds: [`issue-1`],
          nodes: [node()],
        })
      ).toMatchObject({ workflowId: WF, workflowNodeId: `node-1`, workflowRole: `author` })
    })

    it(`loses to a resume's own membership`, () => {
      expect(
        resolveWorkflowMembership({
          parent,
          predecessor: {
            workflowId: DRAFT,
            workflowNodeId: `node-9`,
            workflowRole: `author`,
            startedReason: null,
            parentSessionId: null,
          },
          issueIds: [],
        })
      ).toMatchObject({ workflowId: DRAFT, workflowNodeId: `node-9` })
    })
  })

  describe(`(d) a person's fresh run`, () => {
    it(`on a node's issue joins it as author`, () => {
      expect(resolveWorkflowMembership({ issueIds: [`issue-1`], nodes: [node()] })).toEqual({
        ...none,
        workflowId: WF,
        workflowNodeId: `node-1`,
        workflowRole: `author`,
      })
    })

    it(`on a compound node's member issue joins it`, () => {
      expect(
        resolveWorkflowMembership({
          issueIds: [`sub-2`],
          nodes: [node({ memberIssueIds: [`sub-1`, `sub-2`] })],
        })
      ).toMatchObject({ workflowId: WF, workflowNodeId: `node-1`, workflowRole: `author` })
    })

    it(`joins a draft workflow too`, () => {
      expect(
        resolveWorkflowMembership({
          issueIds: [`issue-1`],
          nodes: [node({ workflowId: DRAFT, workflowStatus: `draft` })],
        })
      ).toMatchObject({ workflowId: DRAFT })
    })

    it(`on an issue of a done, cancelled or paused workflow gets nothing`, () => {
      for (const workflowStatus of [`done`, `cancelled`, `paused`, `failed`]) {
        expect(
          resolveWorkflowMembership({ issueIds: [`issue-1`], nodes: [node({ workflowStatus })] }),
          workflowStatus
        ).toEqual(none)
      }
    })

    it(`prefers the running workflow over a draft one`, () => {
      const nodes = [
        node({
          workflowId: DRAFT,
          nodeId: `node-d`,
          workflowStatus: `draft`,
          workflowCreatedAt: `2026-09-20T00:00:00Z`,
        }),
        node({ nodeId: `node-r`, workflowCreatedAt: `2026-09-01T00:00:00Z` }),
      ]
      expect(resolveWorkflowMembership({ issueIds: [`issue-1`], nodes })).toMatchObject({
        workflowId: WF,
        workflowNodeId: `node-r`,
      })
      // Order-independent.
      expect(
        resolveWorkflowMembership({ issueIds: [`issue-1`], nodes: [...nodes].reverse() })
      ).toMatchObject({ workflowId: WF, workflowNodeId: `node-r` })
    })

    it(`breaks a tie between two drafts by the newest, then the lowest id`, () => {
      const older = node({ workflowId: `wf-a`, workflowStatus: `draft`, workflowCreatedAt: `2026-09-01T00:00:00Z` })
      const newer = node({ workflowId: `wf-b`, workflowStatus: `draft`, workflowCreatedAt: `2026-09-02T00:00:00Z` })
      expect(matchWorkflowNode([`issue-1`], [older, newer])?.workflowId).toBe(`wf-b`)
      const same = node({ workflowId: `wf-c`, workflowStatus: `draft`, workflowCreatedAt: `2026-09-02T00:00:00Z` })
      expect(matchWorkflowNode([`issue-1`], [same, newer])?.workflowId).toBe(`wf-b`)
    })

    it(`joins a batch whose issues all belong to ONE node`, () => {
      const compound = node({ memberIssueIds: [`sub-1`, `sub-2`] })
      expect(
        resolveWorkflowMembership({ issueIds: [`issue-1`, `sub-1`, `sub-2`], nodes: [compound] })
      ).toMatchObject({ workflowId: WF, workflowNodeId: `node-1`, workflowRole: `author` })
    })

    it(`leaves a batch spanning two nodes outside`, () => {
      expect(
        resolveWorkflowMembership({
          issueIds: [`issue-1`, `issue-2`],
          nodes: [node(), node({ nodeId: `node-2`, issueId: `issue-2` })],
        })
      ).toEqual(none)
    })

    it(`keeps the frame's reason`, () => {
      expect(
        resolveWorkflowMembership({ issueIds: [`issue-1`], nodes: [node()], startedReason: `agent` })
      ).toMatchObject({ workflowId: WF, startedReason: `agent` })
    })
  })
})
