//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.3

use sea_orm::entity::prelude::*;
use serde::{Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize)]
#[sea_orm(table_name = "instance_alerts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub host: i32,
    /// number of unhealthy checks after which to alert
    pub host_down_amount: Option<i32>,
    pub host_down_amount_enable: bool,
    /// minimum number of alive accounts under which to alert
    pub alive_accs_min_threshold: Option<i32>,
    pub alive_accs_min_threshold_enable: bool,
    /// minimum percentage of alive accounts under which to aliert
    pub alive_accs_min_percent: Option<i32>,
    pub alive_accs_min_percent_enable: bool,
    /// Avg account age threshold for which to alert when crossed
    pub avg_account_age_days: Option<i32>,
    pub avg_account_age_days_enable: bool,
}

impl Model {
    pub fn gen_defaults(host: i32) -> Self {
        Self {
            host,
            host_down_amount: None,
            host_down_amount_enable: false,
            alive_accs_min_threshold: None,
            alive_accs_min_threshold_enable: false,
            alive_accs_min_percent: None,
            alive_accs_min_percent_enable: false,
            avg_account_age_days: None,
            avg_account_age_days_enable: false,
        }
    }
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
