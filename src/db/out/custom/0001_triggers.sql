-- Custom triggers for Exponential
-- Apply after migrations: docker exec -i exponential-postgres-1 psql -U postgres -d exponential < src/db/out/custom/0001_triggers.sql

-- 1. Auto-update updated_at timestamp on all tables
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspaces FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON projects FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issues FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON labels FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON views FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON push_subscriptions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- 2. Auto-generate issue number and identifier per project
CREATE OR REPLACE FUNCTION generate_issue_number()
RETURNS TRIGGER AS $$
DECLARE
  next_number integer;
  project_prefix text;
BEGIN
  SELECT COALESCE(MAX(number), 0) + 1 INTO next_number
  FROM issues
  WHERE project_id = NEW.project_id;

  SELECT prefix INTO project_prefix
  FROM projects
  WHERE id = NEW.project_id;

  NEW.number := next_number;
  NEW.identifier := project_prefix || '-' || next_number;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER generate_issue_number BEFORE INSERT ON issues FOR EACH ROW EXECUTE FUNCTION generate_issue_number();

-- 3. Auto-populate workspace_id on issue_labels from the referenced label.
-- Keeps the denormalized column consistent so the Electric shape filter
-- on issue_labels can be workspace-scoped (stable) instead of label-scoped
-- (rewritten on every label add → 409 churn → cascading 502s upstream).
CREATE OR REPLACE FUNCTION populate_issue_label_workspace_id()
RETURNS TRIGGER AS $$
BEGIN
  SELECT workspace_id INTO NEW.workspace_id FROM labels WHERE id = NEW.label_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_label_workspace_id BEFORE INSERT ON issue_labels FOR EACH ROW EXECUTE FUNCTION populate_issue_label_workspace_id();
