use serde::{Deserialize};

#[derive(Deserialize)]
pub struct FusionAuthRegistration {
    #[serde(rename = "applicationId")]
    application_id: String,
    pub username: Option<String>
}

#[derive(Deserialize)]
pub struct FusionAuthUser {
    pub email: String,
    pub registrations: Vec<FusionAuthRegistration>
}

impl super::FusionAuthClient {
    pub fn find_auth_registration<'a>(&self, u: &'a FusionAuthUser) -> Option<&'a FusionAuthRegistration> {
        for reg in &u.registrations {
            if reg.application_id == self.cfg.application_id {
                return Some(&reg)
            }
        }
        return None
    }
}