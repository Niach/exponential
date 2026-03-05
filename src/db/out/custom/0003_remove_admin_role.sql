-- Migration: Remove 'admin' from workspace_member_role enum
-- Converts existing admin rows to member, then recreates the enum without admin

BEGIN;

-- 1. Convert any existing admin members to member
UPDATE workspace_members SET role = 'member' WHERE role = 'admin';
UPDATE workspace_invites SET role = 'member' WHERE role = 'admin';

-- 2. Alter columns to text temporarily
ALTER TABLE workspace_members ALTER COLUMN role TYPE text;
ALTER TABLE workspace_invites ALTER COLUMN role TYPE text;

-- 3. Drop old enum and recreate without admin
DROP TYPE IF EXISTS workspace_member_role;
CREATE TYPE workspace_member_role AS ENUM ('owner', 'member');

-- 4. Convert columns back to enum
ALTER TABLE workspace_members ALTER COLUMN role TYPE workspace_member_role USING role::workspace_member_role;
ALTER TABLE workspace_invites ALTER COLUMN role TYPE workspace_member_role USING role::workspace_member_role;

COMMIT;
