use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let cmd = r#"ALTER TABLE "host" ADD COLUMN "connectivity" INT;"#;
        let db = manager.get_connection();
        db.execute_unprepared("BEGIN EXCLUSIVE").await?;
        tracing::info!("adding connectivity column..");
        db.execute_unprepared(cmd).await?;
        db.execute_unprepared("COMMIT TRANSACTION").await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        panic!("Can't migrate down");
    }
}
