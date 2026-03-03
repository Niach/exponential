-- Auto-increment issue numbers per project and generate identifier
CREATE OR REPLACE FUNCTION generate_issue_number()
RETURNS TRIGGER AS $$
DECLARE
  next_number INTEGER;
  project_prefix VARCHAR(10);
BEGIN
  -- Get the next issue number for this project
  SELECT COALESCE(MAX(number), 0) + 1 INTO next_number
  FROM issues
  WHERE project_id = NEW.project_id;

  -- Get the project prefix
  SELECT prefix INTO project_prefix
  FROM projects
  WHERE id = NEW.project_id;

  NEW.number := next_number;
  NEW.identifier := project_prefix || '-' || next_number;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_generate_issue_number
  BEFORE INSERT ON issues
  FOR EACH ROW
  EXECUTE FUNCTION generate_issue_number();

-- Auto-update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_workspaces_updated_at
  BEFORE UPDATE ON workspaces FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_projects_updated_at
  BEFORE UPDATE ON projects FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_issues_updated_at
  BEFORE UPDATE ON issues FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_labels_updated_at
  BEFORE UPDATE ON labels FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_comments_updated_at
  BEFORE UPDATE ON comments FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_attachments_updated_at
  BEFORE UPDATE ON attachments FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_views_updated_at
  BEFORE UPDATE ON views FOR EACH ROW EXECUTE FUNCTION update_updated_at();


CREATE OR REPLACE TRIGGER trg_notifications_updated_at
  BEFORE UPDATE ON notifications FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_issue_relations_updated_at
  BEFORE UPDATE ON issue_relations FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE TRIGGER trg_push_subscriptions_updated_at
  BEFORE UPDATE ON push_subscriptions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
