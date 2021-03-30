-- Your SQL goes here
CREATE TABLE crates (
	"id" TEXT NOT NULL UNIQUE,
	"name" TEXT NOT NULL,
	"updated_at" TEXT NOT NULL,
	"versions" TEXT,
	"keywords" TEXT,
	"categories" TEXT,
	"created_at" TEXT NOT NULL,
	"downloads" INTEGER NOT NULL,
	"recent_downloads" INTEGER NOT NULL,
	"max_version" TEXT NOT NULL,
	"newest_version" TEXT NOT NULL,
	"max_stable_version" TEXT,
	"description" TEXT,
	"homepage" TEXT,
	"documentation" TEXT,
	"repository" TEXT,
	"owners" TEXT,
	PRIMARY KEY("id")
);