use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let columns = [
            "req_photo_rail",
            "req_user_screen_name",
            "req_search",
            "req_list_tweets",
            "req_user_media",
            "req_tweet_detail",
            "req_list",
            "req_user_tweets",
            "req_user_tweets_and_replies",
        ];
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("deleting old instance_stats columns.");
        for column in columns {
            let cmd = format!(r#"ALTER TABLE "instance_stats" DROP COLUMN "{}";"#, column);
            db.execute_unprepared(&cmd).await?;
        }
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
