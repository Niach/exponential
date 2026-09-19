import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/workflow-view.json"
import {
  workflowBand,
  workflowCycleNote,
  workflowEdges,
  workflowNodeCaption,
  workflowNodeTitle,
  workflowNodeTone,
  workflowShapeLine,
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
})
