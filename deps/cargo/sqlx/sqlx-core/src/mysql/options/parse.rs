use crate::error::Error;
use crate::mysql::MySqlConnectOptions;
use std::str::FromStr;
use url::Url;

impl FromStr for MySqlConnectOptions {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let url: Url = s.parse().map_err(Error::config)?;
        let mut options = Self::new();

        if let Some(host) = url.host_str() {
            options = options.host(host);
        }

        if let Some(port) = url.port() {
            options = options.port(port);
        }

        let username = url.username();
        if !username.is_empty() {
            options = options.username(username);
        }

        if let Some(password) = url.password() {
            // Percent decode password in case it contains non-URL safe
            // characters (issues #77, #603)
            let password = percent_encoding::percent_decode_str(password)
                .decode_utf8()
                .map_err(|e| Error::Decode(e.into()))?;

            options = options.password(&*password);
        }

        let path = url.path().trim_start_matches('/');
        if !path.is_empty() {
            options = options.database(path);
        }

        for (key, value) in url.query_pairs().into_iter() {
            match &*key {
                "ssl-mode" => {
                    options = options.ssl_mode(value.parse().map_err(Error::config)?);
                }

                "ssl-ca" => {
                    options = options.ssl_ca(&*value);
                }

                "charset" => {
                    options = options.charset(&*value);
                }

                "collation" => {
                    options = options.collation(&*value);
                }

                "statement-cache-capacity" => {
                    options =
                        options.statement_cache_capacity(value.parse().map_err(Error::config)?);
                }

                "socket" => {
                    options = options.socket(&*value);
                }

                _ => {}
            }
        }

        Ok(options)
    }
}

#[test]
fn percent_decodes_password() {
    let url_str = "mysql://root:aa@bb@localhost/db";
    let options = MySqlConnectOptions::from_str(url_str).unwrap();
    assert_eq!(options.password.as_deref(), Some("aa@bb"));
}
