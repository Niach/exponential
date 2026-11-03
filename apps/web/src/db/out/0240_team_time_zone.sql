-- VAPP-201 (eApps §4.3): the team time zone app hosts read dates in (NULL = each device's own).
ALTER TABLE "teams" ADD COLUMN "time_zone" varchar(64);
