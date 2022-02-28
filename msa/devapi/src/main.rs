mod openapi;
mod shared;
mod auth;
mod api;

use actix_web::{web, App, HttpServer, Result, HttpResponse};
use actix_web::middleware::{Logger, Compress};
use actix_files::NamedFile;
use std::{
    path::PathBuf,
    ffi::OsString,
    sync::Arc,
};
use config::{Config, Environment, File};

pub async fn landing_page() -> Result<NamedFile> {
    let parent_dir: String = std::env::var_os("LANDING_PAGE_DIR").unwrap_or(OsString::from("msa/devapi/ui/landing")).into_string().unwrap();
    let index_file: PathBuf = format!("{}/index.html", &parent_dir).parse()?;
    Ok(NamedFile::open(index_file)?)
}

pub async fn docs_page() -> Result<NamedFile> {
    let parent_dir: String = std::env::var_os("DASHBOARD_PAGE_DIR").unwrap_or(OsString::from("msa/devapi/ui/dashboard")).into_string().unwrap();
    let index_file: PathBuf = format!("{}/doc.html", &parent_dir).parse()?;
    Ok(NamedFile::open(index_file)?)
}

async fn health_check() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().finish())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "info,devapi=debug,actix_web=debug,actix_http=debug");
    env_logger::init();

    // Initialize shared state to access the database as well as any other shared configuration.
    let config: shared::DevApiConfig = Config::builder()
        .add_source(File::with_name("msa/devapi/config/config.toml").required(false))
        .add_source(Environment::with_prefix("squadov"))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();

    let app = Arc::new(shared::SharedApp::new(config.clone()).await);

    HttpServer::new(move || {
        App::new()
            .wrap(Compress::default())
            .wrap(Logger::default())
            .app_data(web::Data::new(app.clone()))
            .service(
                // User-facing protected endpoint.
                // Login via OAuth.
                web::scope("/dashboard")
                    .wrap(auth::oauth::OAuth{config: config.clone()})
                    .route("/", web::get().to(docs_page))
            )
            .service(
                // Machine-facing protected endpoint.
                // Authenticate using API key.
                web::scope("/api")
                    .wrap(auth::api::ApiAuth{app: app.clone()})
                    .service(
                        web::scope("/raw")
                            .route("/wow", web::post().to(api::raw::wow::raw_wow_handler))
                    )
            )
            .service(
                // Publicly facing landing page.
                web::scope("")
                    .route("/swagger/v3/openapi.yml", web::get().to(openapi::openapi_v3))
                    .route("/oauth", web::get().to(auth::oauth::oauth_handler))
                    .route("/healthz", web::get().to(health_check))
                    .route("/", web::get().to(landing_page))
            )
    })
        .bind("0.0.0.0:8080")?
        .run()
        .await
}