# Importing a spreadsheet (CSV, TSV, xlsx) into records

> Generated from docs/spec/apps.md §10 (genUI 0.20.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

## 10. Import and export

| Format | Today | Spec |
|---|---|---|
| `.app.json` (package ± records, signed) | ✅ #VAPP-121, #VAPP-161 (4 MB, ≤ 2,000 records) | Records carry `author`: the export writes it (✅ #VAPP-192, ×4), and the import keeps it when the author is a member of the importing team and the importer an owner; otherwise (a stranger, or a member importing a teammate's export) the row is the importer's (✅ #VAPP-188/192). 🆕 An explicit author mapping when importing into another team. ✅ The file never carries the app's upstream mapping; an owner picks servers in the import step (#VAPP-254, §8). |
| CSV / xlsx export | ❌ | **Out of scope** (decided 2026-11-01). Records leave through `.app.json` and MCP: an agent reads `records_list` or an exposed tool and writes whatever file it needs. |
| CSV / xlsx import | ✅ #VAPP-228: the agent recipe (§10.2, CSV + xlsx) and the Data tab's mapping dialog (§10.3, CSV) | **Agent path** (✅ §10.2): the agent reads the file in a session and calls `records_upsert` with `author` (R2 ✅ #VAPP-188). **Dialog** (✅ §10.3): paste or choose a CSV → map columns onto the Data tab's kind (auto-matched by name) → preview the issues → import the ready rows in one transaction (`apps.records.batch` `via: import`, ≤ 500 rows), Undo for 10 s. |
| JSON (records only) | 🟡 via `records_list` | Covered by `.app.json` |

Example: importing a timesheet workbook (one sheet per month, two members) into the time-tracker app exercises the agent path end to end: on-behalf writes, then the totals computed by the query instead of by spreadsheet formulas. Reports leave over MCP (an exposed tool such as `stunden__month_summary`), not as a file export.

### 10.1 Mapping a spreadsheet onto a kind

One set of rules, shared by the agent recipe and the mapping dialog (fixture `app-records.json` `csvImport`, replayed ×4):

- **Columns to fields.** Compare names with case, spaces and punctuation removed (`Hours worked` = `hoursworked`). A field takes the column with the same name. Failing that, it takes the one column whose name contains the field's name. A column maps to at most one field. Unmapped columns are skipped.
- **Cells to values.** A cell is trimmed first. An empty cell = no value: the field is left out, and a required field without a value is an issue.
  - text: the trimmed text;
  - a string enum: the matching value, ignoring case;
  - number and integer:
    - `6.5` and `6,5` both read 6.5 (a lone comma is a decimal comma);
    - with both `.` and `,`, the LAST one is the decimal separator and the other one groups thousands (`1,240.5`, `1.240,5`);
    - an integer must be whole;
  - boolean: `true/yes/y/1/x` and `false/no/n/0`, ignoring case;
  - `date`: `YYYY-MM-DD`, `DD.MM.YYYY` or `D.M.YYYY`. A slash date (`3/4/2026`) is ambiguous, so it is an issue;
  - `time`: `H:MM` or `HH:MM[:SS]`, padded to `HH:MM` (seconds kept);
  - `date-time`: ISO 8601 only;
  - a list: the cell split on `,` (or `;`), every item read by the item type.

  A cell that does not read as its field's type is an issue naming the column and the row (`row 4: hours: "three" is not a number`). Then the record goes through the kind's schema (`validateRecord`), the same check the server runs.
- **Authors.** A column mapped to the author (owners only, the on-behalf writes of §3.2) matches a member by email, else by name, ignoring case. Any other value is an issue.

### 10.2 The agent recipe (✅ X2 step 1)

When a person attaches a CSV, TSV or xlsx to an editor chat (or any session that may write the app), the agent:

1. **Reads the file.** `exponential_attachments {op: "get", id}`, then download its `downloadUrl` into the working directory and read it with real tooling: for xlsx, a spreadsheet library, one table per sheet. Header rows, merged title rows and total rows are not records.
2. **Picks or declares the kind.** Use an existing kind of the app when its fields fit. Otherwise propose a kind: one field per kept column, typed by the rules above (`date` format for dates, `number` for amounts and hours, an enum for a column with a handful of repeated values). Then save it with `exponential_apps {op: "upsert"}` before writing records.
3. **Dry run.** Map every row with §10.1. Do not write anything yet. Show the person the mapping in ONE `exponential_show` card:
   - the column → field pairs;
   - the first rows as a Table;
   - the count of rows ready;
   - every issue as `row N: field: message`;

   ask them to confirm or correct. A slash date, an unknown member or an ambiguous column is asked about, never guessed.
4. **Writes in batches.** `exponential_apps {op: "records_upsert", slug, kind, data}`, one call per row. Pass `author` (a member's user id from `exponential_people {op: "list"}`) for a row that belongs to someone else (owners only). Send up to 50 calls in parallel per batch, and check each batch's results before the next. Stop at the first refused batch and report it: a schema refusal names the field, and a plan-limit refusal is the team's records quota. The writes land as `via: mcp` in each record's history (§3.5), so a person can see and restore them.
5. **Checks the result.** `exponential_apps {op: "records_list", slug, kind, query}` with the app's own query (a sum per month, a count per member) compared with the spreadsheet's totals. The reply is one line: how many records, and any rows that were skipped.

A NEW app may instead ship its data inside an `.app.json` file with `records` (§10 table, ≤ 2,000 records), which the person imports from the Apps list.

### 10.3 The mapping dialog (✅ X2 step 2)

The Data tab's toolbar has an "Import…" button (×4) next to Add record. It opens the existing import dialog pattern: paste or choose a CSV, map, preview, import. xlsx is not parsed on a client: an xlsx goes through the agent recipe.

- **Reading the file.** CSV per RFC 4180:
  - the delimiter is the most frequent of `,`, `;` and tab in the header line, outside quotes;
  - a BOM is dropped, CRLF and LF both end a row, and quoted fields may hold delimiters, `""` and newlines;
  - blank lines are skipped;
  - the first row is the header. An empty header reads `Column N`, and a repeated one gets ` 2`, ` 3` appended;
  - a row shorter than the header is padded with empty cells. Extra cells beyond the header are an issue.
- **Mapping.** One labelled Select per field of the picked kind (the Data tab's kind, `dataColumns` order). Its options are the columns plus "Skip", and the first pick comes from §10.1. A team owner also gets an Author Select first, defaulting to "Me" (the importer).
- **Preview.** The first rows, each with its issues inline. Below them, a count: `{n} of {total} rows ready`.
- **Import.** Writes the ready rows only (rows with issues are skipped). It is ONE `apps.records.batch` with `via: import`: one transaction, at most `csvImport.maxRows` rows, judged against the team's quota as a whole (over = the plan-limit toast). A success toasts the record count with the existing Undo action for 10 s (§3.5 undo: every created record deleted again).
