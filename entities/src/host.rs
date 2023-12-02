//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "host"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Eq, Serialize)]
#[sea_orm(table_name = "host")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub domain: String,
    pub url: String,
    pub enabled: bool,
    pub rss: bool,
    pub version: Option<String>,
    pub country: String,
    pub version_url: Option<String>,
    pub connectivity: Option<Connectivity>,
    /// Last time the url and enabled were updated, *not* the rss
    pub updated: i64,
    pub account_age_average: Option<i64>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum Connectivity {
    #[sea_orm(num_value = 0)]
    All = 0,
    #[sea_orm(num_value = 1)]
    IPv4 = 1,
    #[sea_orm(num_value = 2)]
    IPv6 = 2,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Domain,
    Url,
    Version,
    Country,
    VersionUrl,
    Enabled,
    Connectivity,
    Rss,
    Updated,
    AccountAgeAverage
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = i32;
    fn auto_increment() -> bool {
        true
    }
}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            // required for updated -> integer
            // generated by inspecting the expanded macro for Model of sea-orm
            Self::Id => ColumnType::Integer.def(),
            Self::Domain => ColumnType::String(None).def(),
            Self::Url => ColumnType::String(None).def(),
            Self::Version => ColumnType::String(None).def().null(),
            Self::Country => ColumnType::String(None).def(),
            Self::VersionUrl => ColumnType::String(None).def().null(),
            Self::Enabled => ColumnType::Integer.def(),
            Self::Rss => ColumnType::Integer.def(),
            Self::Updated => ColumnType::Integer.def(),
            Self::Connectivity => ColumnType::Integer.def().null(),
            Self::AccountAgeAverage => ColumnType::Integer.def().null(),
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::check_errors::Entity")]
    CheckErrors,
    #[sea_orm(has_many = "super::health_check::Entity")]
    HealthCheck,
    #[sea_orm(has_many = "super::instance_alerts::Entity")]
    InstanceAlerts,
    #[sea_orm(has_many = "super::instance_mail::Entity")]
    InstanceMail,
    #[sea_orm(has_many = "super::instance_stats::Entity")]
    InstanceStats,
    #[sea_orm(has_many = "super::mail_verification_tokens::Entity")]
    MailVerificationTokens,
}


impl Related<super::check_errors::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CheckErrors.def()
    }
}

impl Related<super::health_check::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::HealthCheck.def()
    }
}

impl Related<super::instance_alerts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::InstanceAlerts.def()
    }
}

impl Related<super::instance_mail::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::InstanceMail.def()
    }
}

impl Related<super::instance_stats::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::InstanceStats.def()
    }
}

impl Related<super::mail_verification_tokens::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MailVerificationTokens.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}