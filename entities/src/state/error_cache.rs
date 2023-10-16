use chrono::Utc;
use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;

#[derive(Debug, Serialize, Clone, Default)]
pub struct HostError {
    pub time: DateTimeUtc,
    pub message: String,
    pub http_body: Option<String>,
    pub http_status: Option<i32>,
}

impl HostError {
    pub fn new(message: String, http_body: String, http_status: u16) -> Self {
        Self {
            time: Utc::now(),
            message,
            http_body: Some(http_body),
            http_status: Some(http_status as _),
        }
    }

    /// HostError from only a message
    pub fn new_message(message: String) -> Self {
        Self {
            time: Utc::now(),
            message,
            http_body: None,
            http_status: None,
        }
    }

    /// HostError without body
    pub fn new_without_body(message: String, http_status: u16) -> Self {
        Self {
            time: Utc::now(),
            message,
            http_body: None,
            http_status: Some(http_status as _),
        }
    }
}
