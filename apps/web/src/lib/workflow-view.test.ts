import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/workflow-view.json"
import {
  workflowBand,
  workflowCycleNote,
  workflowEdges,
  workflowEdgeStyle,
  workflowFinalPrCaption,
  workflowReviewLine,
  workflowRowSubtitle,
  workflowStartBlocker,
  workflowNodeCaption,
  workflowNodeTitle,
  workflowNodeTone,
  workflowShapeLine,
  NEEDS_YOU_LABEL,
  nodeChipMenu,
  workflowOverflowMenu,
  ALL_NODES_LABEL,
  DECISIONS_LABEL,
  STOP_WORKFLOW_LABEL,
  PICK_DEVICE_LABEL,
  RUNS_ON_LABEL,
  REVIEW_FINAL_PR_LABEL,
  workflowHeaderCaption,
  workflowNodeDisplayLabel,
  workflowNodeDisplayState,
  workflowNodeStrip,
  workflowPrimaryAction,
  type WorkflowPrimaryAction,
} from "./workflow-view"

// EXP-981: what a client says about a workflow, locked ×4 (Android
// WorkflowViewTest, iOS WorkflowViewTests, desktop domain::workflow_view)
// against the ONE contract fixture.
describe(`workflow view (contract fixture)`, () => {
  it(`bands a workflow by its status`, () => {
    for (const c of fixture.bands) expect(workflowBand(c.status)).toBe(c.band)
  })

  it(`says the shape in one line and spells a cycle out`, () => {
    for (const c of fixture.shapeLines) {
      expect(workflowShapeLine(c.metrics)).toBe(c.line)
      expect(workflowCycleNote(c.metrics)).toBe(c.cycleNote)
    }
  })

  it(`captions a node by the plan in a draft and by the state once started`, () => {
    for (const c of fixture.captions) {
      expect(workflowNodeCaption(c.node, c.workflowStatus)).toBe(c.caption)
      expect(workflowNodeTone(c.node.state)).toBe(c.tone)
    }
  })

  it(`titles a compound node with its member count`, () => {
    for (const c of fixture.titles) {
      expect(workflowNodeTitle(c.identifier, c.members)).toBe(c.title)
    }
  })

  for (const c of fixture.edges) {
    it(c.name, () => {
      expect(workflowEdges(c.nodes, c.relations, c.cycleEdges)).toEqual(c.expected)
    })
  }

  it(`says why a draft cannot start, one reason at a time`, () => {
    for (const c of fixture.startBlockers) {
      expect(workflowStartBlocker(c.workflow, c.metrics)).toBe(c.blocker)
    }
  })

  it(`draws the final pull request node once everything landed`, () => {
    for (const c of fixture.finalPr) {
      expect(
        workflowFinalPrCaption(
          c.states.map((state) => ({ state })),
          c.finalPrState,
          c.finalPrNumber
        )
      ).toBe(c.caption)
    }
  })

  it(`leads a list row with the status word where the band is not enough`, () => {
    for (const c of fixture.rowSubtitles) {
      expect(workflowRowSubtitle(c.status, c.metrics)).toBe(c.subtitle)
    }
  })

  it(`styles an edge by what it has to say`, () => {
    for (const c of fixture.edgeStyles) {
      expect(workflowEdgeStyle(c.edge, c.fromState, c.toState)).toBe(c.style)
    }
  })

  it(`says the latest agent review in one line`, () => {
    for (const c of fixture.reviewLines) expect(workflowReviewLine(c.review, c.approved)).toBe(c.line)
  })
})

// EXP-1082 §4: five display states, locked ×4.
describe(`workflow node display states (EXP-1082)`, () => {
  it(`maps every stored state to one of five, an unknown one to queued`, () => {
    for (const c of fixture.displayStates) {
      expect(workflowNodeDisplayState(c.state), c.state).toBe(c.display)
      expect(workflowNodeDisplayLabel(c.state), c.state).toBe(c.caption)
    }
  })

  it(`labels an open question as a badge`, () => {
    expect(NEEDS_YOU_LABEL).toBe(fixture.needsYouLabel)
  })
})

// EXP-1082 §5 / EXP-1066: the page view model, locked ×4.
describe(`workflow page view model (EXP-1082)`, () => {
  it(`lays the node strip out by wave and lane`, () => {
    for (const c of fixture.nodeStrips) {
      expect(
        workflowNodeStrip(c.nodes, c.edges as [string, string][]),
        c.name
      ).toEqual(c.strip)
    }
  })

  it(`captions the header`, () => {
    for (const c of fixture.headerCaptions) {
      expect(workflowHeaderCaption(c.status, c.nodes, c.device), c.name).toBe(c.caption)
    }
  })

  it(`picks the one primary action`, () => {
    for (const c of fixture.primaryActions) {
      expect(workflowPrimaryAction(c.status, c.device)).toBe(
        c.action as WorkflowPrimaryAction | null
      )
    }
  })

  it(`offers retry and skip on a failed chip only`, () => {
    for (const c of fixture.chipMenus) {
      expect(nodeChipMenu(c.state)).toEqual(c.menu)
    }
  })

  it(`fills the header's overflow by status`, () => {
    for (const c of fixture.overflowMenus) {
      expect(workflowOverflowMenu(c.status), c.status).toEqual(c.menu)
    }
  })

  it(`spells the page labels the same everywhere`, () => {
    expect({
      allNodes: ALL_NODES_LABEL,
      decisions: DECISIONS_LABEL,
      stop: STOP_WORKFLOW_LABEL,
      pickDevice: PICK_DEVICE_LABEL,
      runsOn: RUNS_ON_LABEL,
      reviewFinalPr: REVIEW_FINAL_PR_LABEL,
    }).toEqual(fixture.pageLabels)
  })
})
