// VAPP-202: the page module of the web runners' `records-25k` (web-shared.ts
// bundles it): the rows built outside the clock, the TS records query timed
// with `performance.now()` in Chromium.
import { recordRows, runRecordsQueries, type RecordsSpec, type RecordsSteps } from "../src/records"

window.__xuiRecordsBench = (spec: RecordsSpec, steps: RecordsSteps) => runRecordsQueries(recordRows(spec), spec.kind, steps, () => performance.now())
