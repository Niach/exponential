-- Auto-update updated_at for workspace_members
CREATE OR REPLACE TRIGGER trg_workspace_members_updated_at
  BEFORE UPDATE ON workspace_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Auto-update updated_at for workspace_invites
CREATE OR REPLACE TRIGGER trg_workspace_invites_updated_at
  BEFORE UPDATE ON workspace_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();
