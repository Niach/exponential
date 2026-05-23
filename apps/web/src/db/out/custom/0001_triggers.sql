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
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_agents FOR EACH ROW EXECUTE FUNCTION update_updated_at();

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

-- 3. Bump issue.updated_at when a comment is created/edited/deleted so the
--    companion daemon's Electric shape on `issues` fires an `updated` event
--    on new discussion. The daemon uses this to re-enter the pipeline (e.g.,
--    to revise a plan in light of new comments).
CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()
RETURNS TRIGGER AS $$
DECLARE
  target_issue uuid;
BEGIN
  IF (TG_OP = 'DELETE') THEN
    target_issue := OLD.issue_id;
  ELSE
    target_issue := NEW.issue_id;
  END IF;
  UPDATE issues SET updated_at = now() WHERE id = target_issue;
  IF (TG_OP = 'DELETE') THEN
    RETURN OLD;
  ELSE
    RETURN NEW;
  END IF;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER bump_issue_updated_at_from_comment
  AFTER INSERT OR UPDATE OR DELETE ON comments
  FOR EACH ROW EXECUTE FUNCTION bump_issue_updated_at_from_comment();
