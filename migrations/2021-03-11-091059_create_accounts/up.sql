-- Your SQL goes here
CREATE TABLE accounts (
	"id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
	"username" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "salt" TEXT NOT NULL,
	"email" TEXT,
	"type" TEXT NOT NULL,
	"role" TEXT NOT NULL,
    "password" TEXT NOT NULL,
	"created_at" DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
	"last_login" TEXT,
	"token" TEXT
);