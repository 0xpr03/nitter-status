// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::Form;
use chrono::Duration;
use chrono::Local;
use chrono::Utc;
use entities::mail_verification_tokens;
use entities::state::AppState;
use lettre::message::Mailbox;
use lettre::Message;
use lettre::SmtpTransport;
use lettre::Transport;
use rand::distributions::Alphanumeric;
use rand::distributions::DistString;
use reqwest::Url;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::ModelTrait;
use sea_orm::QueryFilter;
use sea_orm::Related;
use sea_orm::TransactionTrait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tower_sessions::Session;

use super::get_specific_login_host;
use super::Result;
use entities::instance_alerts;
use entities::instance_mail;

pub async fn view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let host = get_specific_login_host(instance, &session, db).await?;
    let mail = host.find_related(instance_mail::Entity).one(db).await?;
    let alerts = host.find_related(instance_alerts::Entity).one(db).await?;
    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    context.insert("HOST_ID", &instance);
    context.insert("MAIL", &mail);
    context.insert("ALERTS", &alerts);

    let res = Html(template.render("alerts.html.j2", &context)?).into_response();
    Ok(res)
}

/// Admin login form
#[derive(Deserialize, Debug)]
pub struct AddEmailForm {
    mail: String,
}
//let verifier_entry = mail_verification_tokens::Entity::find().filter(mail_verification_tokens::Column::KnownPart.eq(input.))
pub async fn add_mail(
    State(ref config): State<Arc<crate::Config>>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
    Form(input): Form<AddEmailForm>,
) -> Result<axum::response::Response> {
    let back_url: String = back_url_host_alerts(instance);

    let transaction = db.begin().await?;

    let host = get_specific_login_host(instance, &session, &transaction).await?;

    let mail = host
        .find_related(instance_mail::Entity)
        .one(&transaction)
        .await?;

    if mail.is_some() {
        transaction.rollback().await?;
        return super::render_error_page(
            template,
            "Invalid operation",
            "Can't add another email, please remove the current one.",
            &back_url,
        );
    }

    let (secret, new_token_model) = generate_mail_token(host.id, config.mail_token_ttl_s);
    let token_model = new_token_model.insert(&transaction).await?;

    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    let mut url = Url::parse(&config.site_url).expect("invalid site url");
    url.set_path(&format!(
        "/admin/mail/activate/{}/{}",
        token_model.known_part, secret
    ));
    context.insert("ACTIVATION_LINK", url.as_str());

    let mail_body = template.render("mail_activation.j2", &context)?;

    let address: Mailbox = match input.mail.parse() {
        Ok(v) => v,
        Err(e) => {
            transaction.rollback().await?;
            tracing::info!(error=?e, address=input.mail,"Failed to parse email address");
            return super::render_error_page(
                template,
                "Invalid email address",
                "Your email address seems to be invalid.",
                &back_url,
            );
        }
    };

    let email = Message::builder()
        // Addresses can be specified by the tuple (email, alias)
        .to(address)
        // ... or by an address only
        .from(config.mail_from.parse()?)
        .subject(format!("Mail Activation for {}", config.site_url))
        .body(mail_body)?;

    let smtp_credentials = lettre::transport::smtp::authentication::Credentials::new(
        config.mail_smtp_user.clone(),
        config.mail_smtp_password.clone(),
    );

    // Open a local connection on port 25
    let mailer = SmtpTransport::relay(&config.mail_smtp_host)
        .unwrap()
        .credentials(smtp_credentials)
        .build();
    // Send the email
    match mailer.send(&email) {
        Ok(_) => (),
        Err(e) => {
            tracing::info!(error=?e, address=input.mail,"Failed to send validation mail");
            return super::render_error_page(
                template,
                "Failed to send email",
                "Couldn't send email.",
                &back_url,
            );
        }
    }

    transaction.commit().await?;

    let mut context = tera::Context::new();
    context.insert("EMAIL", &host.domain);
    context.insert("MAIL_VALID_UNTIl", );
    context.insert("URL_BACK", &back_url);

    let res = Html(template.render("mail_send.html.j2", &context)?).into_response();
    Ok(res)
}

fn generate_mail_token(
    host: i32,
    lifetime_s: i64,
) -> (String, mail_verification_tokens::ActiveModel) {
    let public = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    let secret = Alphanumeric.sample_string(&mut rand::thread_rng(), 20);

    let secret_hashed_encoded: String = {
        let mut hasher: Sha256 = Sha256::new();
        hasher.update(&secret);
        let res = hasher.finalize();
        let mut secret_hashed = [0; 32];
        base16ct::upper::encode_string(&secret_hashed)
    };

    let eol = Utc::now() + Duration::seconds(lifetime_s);

    (
        secret,
        mail_verification_tokens::ActiveModel {
            host: ActiveValue::Set(host),
            known_part: ActiveValue::Set(public),
            secret_part: ActiveValue::Set(secret_hashed_encoded),
            eol_date: ActiveValue::Set(eol.timestamp()),
        },
    )
}

fn back_url_host_alerts(instance: i32) -> String {
    format!("/admin/alerts/{instance}")
}
