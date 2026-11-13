// #EXP-1617: GitHub workflow YAML TEXT through the reader (workflow.json
// `text` cases): YAML 1.2 core schema (`on` stays a plain key, only
// true/false are booleans), strict, unique keys; text that does not parse =
// one `yaml` error at the root. The server plans with this; ecode's
// `workflows` feature is the same step for a host without a server. Kept out
// of the package index so a browser import never pulls a YAML parser in.
import { parse as parseYaml } from "yaml"
import { readWorkflow, type WorkflowRead } from "./workflow"

export function readWorkflowText(text: string): WorkflowRead {
  let parsed: unknown
  try {
    parsed = parseYaml(text, { schema: `core`, strict: true, uniqueKeys: true })
  } catch {
    return { file: null, errors: [{ path: ``, code: `yaml` }] }
  }
  return readWorkflow(parsed)
}
