use squadov_common::SquadOvError;
use actix_web::{web, web::BufMut, HttpResponse, HttpRequest, HttpMessage};
use actix_multipart::Multipart;
use crate::api;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};
use chrono::Utc;
use reqwest::header;

#[derive(Deserialize)]
struct GitlabUploadFileResult {
    markdown: String
}

impl api::ApiApplication {
    async fn submit_bug_report(&self, title: &str, description: &str, log_bytes: web::Bytes, user_id: i64) -> Result<(), SquadOvError> {
        let mut headers = header::HeaderMap::new();
        headers.insert("PRIVATE-TOKEN", header::HeaderValue::from_str(&self.config.gitlab.access_token)?);

        // Upload the file to gitlab. Put the markdown of the upload into the description that the user gave us.
        let gitlab_client = reqwest::Client::builder().default_headers(headers).build()?;
        let timestamp = Utc::now().to_rfc3339();
        let fname = format!("logs-{}-{}.zip", user_id, &timestamp);
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::stream(log_bytes)
                    .file_name(fname.clone())
            );

        let file_upload_result = gitlab_client
            .post(&format!("https://gitlab.com/api/v4/projects/{}/uploads", self.config.gitlab.project_id))
            .multipart(form)
            .send()
            .await?;

        let status = file_upload_result.status().as_u16();
        if status != 201 {
            return Err(SquadOvError::InternalError(format!("Failed to upload Gitlab logs [{}]: {}", status, file_upload_result.text().await?)));
        }

        let file_upload_result = file_upload_result
            .json::<GitlabUploadFileResult>()
            .await?;

        #[derive(Serialize)]
        struct BugReportPacket {
            created_at: String,
            title: String,
            description: String,
            labels: String,
        }

        let issue_result = gitlab_client
            .post(&format!("https://gitlab.com/api/v4/projects/{project_id}/issues",
                project_id=self.config.gitlab.project_id,
            ))
            .json(&BugReportPacket{
                created_at: timestamp.clone(),
                labels: "bug,user-reported".to_string(),
                title: format!("[USER REPORTED BUG] {title}", title=title),
                description: format!(r#"
USER ID: {user_id}

LOGS: {log}

DESCRIPTION: {description}
                "#, user_id=user_id, log=&file_upload_result.markdown, description=description),
            })
            .send()
            .await?;

        let status = issue_result.status().as_u16();
        if status != 201 {
            return Err(SquadOvError::InternalError(format!("Failed to create Gitlab issue [{}]: {}", status, issue_result.text().await?)));
        }

        Ok(())
    }
}

pub async fn create_bug_report_handler(app : web::Data<Arc<api::ApiApplication>>, mut payload: Multipart, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };
    
    let mut title = web::BytesMut::new();
    let mut description = web::BytesMut::new();
    let mut logs = web::BytesMut::new();

    while let Some(mut field) = payload.try_next().await? {
        let field_name = String::from(field.content_disposition().get_name().ok_or(SquadOvError::BadRequest)?);

        let mut tmp = web::BytesMut::new();
        while let Some(Ok(chunk)) = field.next().await {
            tmp.put(&*chunk);
        }

        match field_name.as_str() {
            "title" => title.put(&*tmp),
            "description" => description.put(&*tmp),
            "logs" => logs.put(&*tmp),
            _ => return Err(SquadOvError::BadRequest),
        }
    }

    app.submit_bug_report(
        std::str::from_utf8(&*title)?,
        std::str::from_utf8(&*description)?,
        logs.freeze(),
        session.user.id,
    ).await?;
    Ok(HttpResponse::Ok().finish())
}