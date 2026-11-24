-- #EXP-2082: IF NOT EXISTS: staging applied this column as an earlier 0294 before master took that number.
ALTER TABLE "installs" ADD COLUMN IF NOT EXISTS "app_version" varchar(32);
