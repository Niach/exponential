ALTER TABLE "comments" DROP CONSTRAINT "comments_kind_regular";--> statement-breakpoint
ALTER TABLE "comments" DROP COLUMN "kind";--> statement-breakpoint
ALTER TABLE "comments" DROP COLUMN "answered_at";