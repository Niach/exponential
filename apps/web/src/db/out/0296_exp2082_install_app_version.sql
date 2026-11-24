-- #EXP-2082: IF NOT EXISTS: staging applied this column under earlier numbers (0294, 0295) before master took them.
ALTER TABLE "installs" ADD COLUMN IF NOT EXISTS "app_version" varchar(32);
