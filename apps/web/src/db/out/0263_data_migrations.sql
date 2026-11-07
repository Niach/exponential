CREATE TABLE "data_migrations" (
	"name" text PRIMARY KEY NOT NULL,
	"finished_at" timestamp with time zone DEFAULT now() NOT NULL,
	"report" jsonb NOT NULL
);
