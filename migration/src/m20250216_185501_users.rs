use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd_user = r#"CREATE TABLE "user" (
            "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
            "username" text NOT NULL UNIQUE,
            "admin" integer NOT NULL,
            "password" text NOT NULL,
            "login_time" integer NOT NULL,
            "disabled" integer NOT NULL
        ) STRICT;"#;
        let cmd_access = r#"CREATE TABLE "access" (
            "user" integer NOT NULL,
            "host" text NOT NULL,
            "write" integer NOT NULL,
            CONSTRAINT "pk_access_userhost" PRIMARY KEY ("user", "host"),
            FOREIGN KEY ("user") REFERENCES "user" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) STRICT;"#;
        let cmd_log = r#"CREATE TABLE "log" (
            "user" integer NOT NULL,
            "host" integer,
            "key" text NOT NULL,
            "time" integer NOT NULL,
            "old_value" text NOT NULL,
            "new_value" text NOT NULL,
            CONSTRAINT "pk_log_usertime" PRIMARY KEY ("user", "time"),
            FOREIGN KEY ("user") REFERENCES "user" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) STRICT;"#;
        let cmd_log_index = r#"CREATE INDEX "index_log_time" ON log ("time");"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding user table..");
        db.execute_unprepared(cmd_user).await?;
        tracing::info!("adding access table..");
        db.execute_unprepared(cmd_access).await?;
        tracing::info!("adding log table..");
        db.execute_unprepared(cmd_log).await?;
        tracing::info!("adding log index..");
        db.execute_unprepared(cmd_log_index).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
