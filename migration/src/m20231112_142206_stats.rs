use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd = r#"CREATE TABLE "instance_stats" (
            "time" integer NOT NULL,
            "host" integer NOT NULL,
            "limited_accs" integer NOT NULL,
            "total_accs" integer NOT NULL,
            "total_requests" integer NOT NULL,
            "req_photo_rail" integer NOT NULL,
            "req_user_screen_name" integer NOT NULL,
            "req_search" integer NOT NULL,
            "req_list_tweets" integer NOT NULL,
            "req_user_media" integer NOT NULL,
            "req_tweet_detail" integer NOT NULL,
            "req_list" integer NOT NULL,
            "req_user_tweets" integer NOT NULL,
            "req_user_tweets_and_replies" integer NOT NULL,
            CONSTRAINT "pk_instance_stats" PRIMARY KEY ("host", "time"),
            FOREIGN KEY ("host") REFERENCES "host" ("id") ON DELETE CASCADE ON UPDATE CASCADE
        ) WITHOUT ROWID, STRICT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding instance_stats table..");
        db.execute_unprepared(cmd).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
