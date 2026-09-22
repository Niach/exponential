import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/workflow-view.json"
import {
  workflowBand,
  workflowCycleNote,
  workflowEdges,
  workflowEdgeStyle,
  workflowFinalPrCaption,
  workflowMergeTrain,
  workflowMetricRows,
  workflowReviewLine,
  workflowRowSubtitle,
  workflowStartBlocker,
  workflowTrainStepLabel,
  type TrainStep,
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

  it(`says why a draft cannot start, one reason at a time`, () => {
    for (const c of fixture.startBlockers) {
      expect(workflowStartBlocker(c.workflow, c.metrics)).toBe(c.blocker)
    }
  })

  for (const c of fixture.trains) {
    it(c.name, () => {
      expect(workflowMergeTrain(c.nodes)).toEqual(c.expected)
    })
  }

  it(`labels every train step`, () => {
    for (const [step, label] of Object.entries(fixture.trainStepLabels)) {
      expect(workflowTrainStepLabel(step as TrainStep)).toBe(label)
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

  it(`lists the metrics that have something to say`, () => {
    for (const c of fixture.metricRows) {
      expect(workflowMetricRows(c.metrics as Record<string, unknown>)).toEqual(c.rows)
    }
  })
})
