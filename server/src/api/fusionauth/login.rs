use serde::{Deserialize, Serialize};
use derive_more::{Display};
use squadov_common::SquadOvError;

#[derive(Serialize)]
pub struct FusionAuthLoginInput {
    #[serde(rename = "applicationId")]
    application_id: String,
    #[serde(rename = "ipAddress")]
    ip_address: String,
    #[serde(rename = "loginId")]
    username: String,
    #[serde(rename = "password")]
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct FusionAuthLoginResult {
    pub user: super::FusionAuthUser,
    pub token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String
}

#[derive(Debug, Display)]
pub enum FusionAuthLoginError {
    Auth,
    WrongApp,
    ChangePassword(String),
    TwoFactor(String),
    InternalError,
    #[display(fmt = "Generic: {} {}", code, message)]
    Generic {
        code: u16,
        message: String    
    }
}

#[derive(Deserialize,Debug)]
struct FusionAuthLoginChangePassword {
    #[serde(rename = "changePasswordId")]
    change_password_id: String,
}

#[derive(Deserialize,Debug)]
struct FusionAuthLoginTwoFactor {
    #[serde(rename = "twoFactorId")]
    two_factor_id: String
}

#[derive(Serialize,Debug,Clone)]
#[serde(rename_all="camelCase")]
pub struct FusionAuthMfaLoginInput {
    code: String,
    two_factor_id: String,
    application_id: String,
}

impl super::FusionAuthClient {
    pub fn build_login_input(&self, username : String, password: String, ip : Option<&str>) -> FusionAuthLoginInput {
        return FusionAuthLoginInput{
            application_id: self.cfg.application_id.clone(),
            ip_address: match ip {
                Some(x) => String::from(x),
                None => String::from(""),  
            },
            username: username,
            password: password,
        }
    }

    pub async fn logout(&self, refresh_token: &str) -> Result<(), FusionAuthLoginError> {
        let res = self.client.post(self.build_url(format!("/api/logout?refreshToken={}", refresh_token).as_str()).as_str())
            .send()
            .await;

        match res {
            Ok(resp) => {
                let status = resp.status();
                match status.as_u16() {
                    200 => Ok(()),
                    500 => Err(FusionAuthLoginError::InternalError),
                    _ => Err(FusionAuthLoginError::Generic{
                        code: status.as_u16(),
                        message: format!("Fusion Auth Error: {}", resp.text().await.unwrap()),
                    })
                }
            },
            Err(err) => Err(FusionAuthLoginError::Generic{
                code: 0,
                message: format!("{}", err)
            }),
        }
    }

    pub async fn mfa_login(&self, id: &str, code: &str) -> Result<FusionAuthLoginResult, SquadOvError> {
        let resp = self.client.post(self.build_url("/api/two-factor/login").as_str())
            .json(&FusionAuthMfaLoginInput{
                code: code.to_string(),
                two_factor_id: id.to_string(),
                application_id: self.cfg.application_id.clone()
            })
            .send()
            .await?;

        match resp.status().as_u16() {
            200 | 212 | 213 => Ok(resp.json::<FusionAuthLoginResult>().await?),
            _ => Err(SquadOvError::InternalError(format!("FA MFA Login Internal Error {} - {}", resp.status().as_u16(), resp.text().await?)))
        }
    }

    pub async fn login(&self, input : FusionAuthLoginInput) -> Result<FusionAuthLoginResult, FusionAuthLoginError> {
        let res = self.client.post(self.build_url("/api/login").as_str())
            .json(&input)
            .send()
            .await;

        match res {
            Ok(resp) => {
                let status = resp.status();

                match status.as_u16() {
                    200 | 212 | 213 => {
                        let body = resp.json::<FusionAuthLoginResult>().await;
                        match body {
                            Ok(j) => Ok(j),
                            Err(err) => Err(FusionAuthLoginError::Generic{
                                code: 0,
                                message: format!("{}", err)
                            }),
                        }
                    }
                    202 => Err(FusionAuthLoginError::WrongApp),
                    203 => {
                        let body = resp.json::<FusionAuthLoginChangePassword>().await;
                        match body {
                            Ok(j) => Err(FusionAuthLoginError::ChangePassword(j.change_password_id)),
                            Err(err) => Err(FusionAuthLoginError::Generic{
                                code: 0,
                                message: format!("{}", err)
                            }),
                        }
                    },
                    242 => {
                        let body = resp.json::<FusionAuthLoginTwoFactor>().await;
                        match body {
                            Ok(j) => Err(FusionAuthLoginError::TwoFactor(j.two_factor_id)),
                            Err(err) => Err(FusionAuthLoginError::Generic{
                                code: 0,
                                message: format!("{}", err)
                            }),
                        }
                    },
                    404 => Err(FusionAuthLoginError::Auth),
                    _ => Err(FusionAuthLoginError::Generic{
                        code: status.as_u16(),
                        message: format!("Fusion Auth Error: {}", resp.text().await.unwrap()),
                    })
                }
            },
            Err(err) => Err(FusionAuthLoginError::Generic{
                code: 0,
                message: format!("{}", err)
            }),
        }
    }
}