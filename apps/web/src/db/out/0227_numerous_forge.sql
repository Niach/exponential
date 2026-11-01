CREATE TABLE "eagent_shadow_counts" (
	"rule" varchar(64) NOT NULL,
	"day" date NOT NULL,
	"compared" integer DEFAULT 0 NOT NULL,
	"mismatched" integer DEFAULT 0 NOT NULL,
	"skipped" integer DEFAULT 0 NOT NULL,
	"failed" integer DEFAULT 0 NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "eagent_shadow_counts_rule_day_pk" PRIMARY KEY("rule","day")
);
--> statement-breakpoint
CREATE TABLE "eagent_shadow_samples" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"rule" varchar(64) NOT NULL,
	"diff" jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE INDEX "idx_eagent_shadow_samples_rule_created" ON "eagent_shadow_samples" USING btree ("rule","created_at");