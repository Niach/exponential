-- Auto-populate workspace_id on issue_labels from the referenced label,
-- so the Electric shape filter on issue_labels can be workspace-scoped
-- (stable) instead of label-scoped (rewritten on every label add → 409
-- churn → cascading 502s upstream). Runs as part of the regular drizzle
-- migration pipeline so deploys do not require a manual psql step.
CREATE OR REPLACE FUNCTION populate_issue_label_workspace_id()
RETURNS TRIGGER AS $$
BEGIN
  SELECT workspace_id INTO NEW.workspace_id FROM labels WHERE id = NEW.label_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_issue_label_workspace_id BEFORE INSERT ON issue_labels FOR EACH ROW EXECUTE FUNCTION populate_issue_label_workspace_id();
