// EXP-1153: the issue view — one issue with its comments, relations and PR.

import type { ViewProps } from "../shell"
import type { IssueViewData } from "../contract"
import { IssueDetail } from "./issue-detail"

export function IssueView({ data, bridge }: ViewProps<IssueViewData>) {
  return <IssueDetail data={data} bridge={bridge} />
}
