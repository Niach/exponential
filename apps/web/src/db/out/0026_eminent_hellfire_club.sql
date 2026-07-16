ALTER TYPE "public"."issue_status" ADD VALUE 'in_review' BEFORE 'done';--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "recurrence_interval";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "recurrence_unit";--> statement-breakpoint
DROP TYPE "public"."recurrence_unit";