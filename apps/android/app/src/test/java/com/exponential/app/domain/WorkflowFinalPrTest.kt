package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-1072: a workflow's final PR is named identically on every client
// (web `lib/workflow-final-pr-identity.ts`).
class WorkflowFinalPrTest {
    @Test
    fun identifierNamesTheWorkflow() {
        assertEquals("Workflow: EXP-996 +5", WorkflowFinalPr.identifier("EXP-996 +5"))
    }

    @Test
    fun pickLabelLeadsWithThePrNumber() {
        assertEquals("#829 · Workflow: EXP-996 +5", WorkflowFinalPr.pickLabel(829, "EXP-996 +5"))
    }

    @Test
    fun pickLabelWithoutANumberIsTheIdentifier() {
        assertEquals("Workflow: Launch", WorkflowFinalPr.pickLabel(null, "Launch"))
    }

    @Test
    fun reviewKeyIsNamespacedByWorkflowId() {
        assertEquals("workflow:wf-1", WorkflowFinalPr.reviewKey("wf-1"))
    }
}
