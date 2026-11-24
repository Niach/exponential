import { IssueDetailView } from "../issue-detail-view"
import { IssueListView } from "../issue-list-view"
import { mount } from "../mount"

mount(`issues`, { IssueDetailView, IssueListView })
