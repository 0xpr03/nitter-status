//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use sea_orm::{entity::prelude::*, FromQueryResult};
use sea_query::{Alias, Order, Query, SimpleExpr};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize)]
#[sea_orm(table_name = "instance_stats")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub time: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub host: i32,
    pub limited_accs: i32,
    pub total_accs: i32,
    pub total_requests: i64,
    pub req_photo_rail: i32,
    pub req_user_screen_name: i32,
    pub req_search: i32,
    pub req_list_tweets: i32,
    pub req_user_media: i32,
    pub req_tweet_detail: i32,
    pub req_list: i32,
    pub req_user_tweets: i32,
    pub req_user_tweets_and_replies: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::host::Entity",
        from = "Column::Host",
        to = "super::host::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Host,
}

impl Related<super::host::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Host.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Debug, FromQueryResult, Serialize)]
pub struct StatsAmount {
    pub time: i64,
    pub limited_accs_max: i32,
    pub limited_accs_avg: i32,
    pub total_accs_max: i32,
    pub total_accs_avg: i32,
    pub total_requests_max: i64,
    pub total_requests_avg: i64,
    pub req_photo_rail_max: i32,
    pub req_photo_rail_avg: i32,
    pub req_user_screen_name_max: i32,
    pub req_user_screen_name_avg: i32,
    pub req_search_max: i32,
    pub req_search_avg: i32,
    pub req_list_tweets_max: i32,
    pub req_list_tweets_avg: i32,
    pub req_user_media_max: i32,
    pub req_user_media_avg: i32,
    pub req_tweet_detail_max: i32,
    pub req_tweet_detail_avg: i32,
    pub req_list_max: i32,
    pub req_list_avg: i32,
    pub req_user_tweets_max: i32,
    pub req_user_tweets_avg: i32,
    pub req_user_tweets_and_replies_max: i32,
    pub req_user_tweets_and_replies_avg: i32,
}

impl StatsAmount {
    /// Fetch health check graph data for all or selected hosts in the selected time range.
    pub async fn fetch(
        db: &DatabaseConnection,
        from: DateTimeUtc,
        to: DateTimeUtc,
        hosts: Option<&[i32]>,
    ) -> Result<Vec<StatsAmount>, DbErr> {
        let builder = db.get_database_backend();
        let columns = [
            "limited_accs",
            "total_accs",
            "total_requests",
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
        let mut stmt: sea_query::SelectStatement = Query::select();
        let col_stmt = stmt.column(self::Column::Time);
        for col in columns {
            col_stmt
                .expr_as(
                    SimpleExpr::Custom(format!("MAX({col})")),
                    Alias::new(format!("{col}_max")),
                )
                .expr_as(
                    SimpleExpr::Custom(format!("CAST(ifnull(AVG({col}),0) as int)")),
                    Alias::new(format!("{col}_avg")),
                );
        }
        col_stmt
            .group_by_col(self::Column::Time)
            .from(self::Entity)
            .and_where(self::Column::Time.between(from.timestamp(), to.timestamp()));
        if let Some(hosts) = hosts {
            stmt.and_where(self::Column::Host.is_in(hosts.iter().map(|v| *v)));
        }
        stmt.group_by_col(self::Column::Time)
            .order_by(self::Column::Time, Order::Asc);
        StatsAmount::find_by_statement(builder.build(&stmt))
            .all(db)
            .await
    }
}

#[derive(Debug, FromQueryResult, Serialize)]
pub struct StatsCSVEntry {
    pub time: i64,
    pub limited_accs_avg: i32,
    pub total_accs_avg: i32,
    pub total_requests_avg: i64,
}

impl StatsCSVEntry {
    /// Fetch health check graph data for all or selected hosts in the selected time range.
    pub async fn fetch(db: &DatabaseConnection) -> Result<Vec<Self>, DbErr> {
        let builder = db.get_database_backend();
        let columns = ["limited_accs", "total_accs", "total_requests"];
        let mut stmt: sea_query::SelectStatement = Query::select();
        stmt.column(self::Column::Time);
        for col in columns {
            stmt.expr_as(
                SimpleExpr::Custom(format!("CAST (AVG({col}) as int)")),
                Alias::new(format!("{col}_avg")),
            );
        }
        stmt.group_by_col(self::Column::Time).from(self::Entity);
        stmt.group_by_col(self::Column::Time)
            .order_by(self::Column::Time, Order::Asc);
        Self::find_by_statement(builder.build(&stmt)).all(db).await
    }
}
