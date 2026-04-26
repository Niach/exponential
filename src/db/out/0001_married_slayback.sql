CREATE TYPE "public"."recurrence_unit" AS ENUM('day', 'week', 'month');--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "recurrence_interval" integer;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "recurrence_unit" "recurrence_unit";