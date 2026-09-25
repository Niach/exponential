import Foundation

// EXP-1072: how a workflow's ONE final pull request (integration branch → the
// default branch) is NAMED wherever a PR is shown — Reviews, the `pr` input
// picker. It is the workflow's OWN linked PR, never an "external" one.
// Byte-identical to apps/web/src/lib/workflow-final-pr-identity.ts.
public enum WorkflowFinalPr {
    /// The final PR's title on GitHub, and the `pr` input's "identifier" for it.
    public static func identifier(name: String) -> String {
        "Workflow: \(name)"
    }

    /// `#829 · Workflow: EXP-996 +5` — the PR number when known, then the
    /// identifier (the `StartPullRequestOption.label` shape).
    public static func pickLabel(number: Int?, name: String) -> String {
        let identifier = identifier(name: name)
        guard let number else { return identifier }
        return "#\(number) · \(identifier)"
    }

    /// The Reviews queue's entry key for a workflow's final PR.
    public static func reviewKey(workflowId: String) -> String {
        "workflow:\(workflowId)"
    }
}
