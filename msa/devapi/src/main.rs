mod openapi;

use actix_web::{web, App, HttpServer};
use actix_web::middleware::{Logger};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "info,devapi=debug,actix_web=debug,actix_http=debug");
    env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .service(
                // Machine-facing protected endpoint.
                // Authenticate using API key.
                web::scope("/api")
            )
            .route("/swagger/v3/openapi.yml", web::get().to(openapi::openapi_v3))
    })
        .bind("0.0.0.0:8080")?
        .run()
        .await
}