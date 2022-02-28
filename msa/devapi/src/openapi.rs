use actix_files::NamedFile;
use actix_web::Result;
use std::{
    path::PathBuf,
    ffi::OsString,
};

pub async fn openapi_v3() -> Result<NamedFile> {
    let path: PathBuf = std::env::var_os("OPENAPI_FILE").unwrap_or(OsString::from("msa/devapi/openapi/devapi.yml")).into_string().unwrap().parse()?;
    Ok(NamedFile::open(path)?)
}