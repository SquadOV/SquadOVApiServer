use crate::SquadOvError;
use rusoto_core::{Region, HttpClient};
use rusoto_s3::S3Client;
use rusoto_cognito_identity::CognitoIdentityClient;
use rusoto_credential::ProfileProvider;
use serde::{Deserialize};
use rsa::{
    RsaPrivateKey,
    pkcs1::FromRsaPrivateKey,
    padding::PaddingScheme,
    hash::Hash
};
use sha1::Digest;
use chrono::{Utc, Duration};
use std::str::FromStr;

#[derive(Deserialize,Debug,Clone)]
pub struct AWSCDNConfig {
    pub public_cdn_domain: String,
    pub private_cdn_domain: String,
    pub blob_cdn_domain: String,
    pub public_key_id: String,
    pub private_key_fname: String,
}

#[derive(Deserialize,Debug,Clone)]
pub struct AWSCognitoConfig {
    pub pool_id: String,
    pub provider: String,
}

#[derive(Deserialize,Debug,Clone)]
pub struct AWSConfig {
    pub enabled: bool,
    pub credential_path: String,
    pub profile: String,
    pub cdn: AWSCDNConfig,
    pub region: String,
    pub account_id: String,
    pub cognito: AWSCognitoConfig,
}

pub struct AWSClient {
    pub region: Region,
    pub provider: ProfileProvider,
    pub s3: S3Client,
    pub cognito: CognitoIdentityClient,
    pub config: AWSConfig,
    cdn_private_key: RsaPrivateKey,
}

impl AWSClient {
    pub fn new(config: &AWSConfig) -> Self {
        let provider = ProfileProvider::with_configuration(&config.credential_path, &config.profile);
        let region = Region::from_str(config.region.as_str()).unwrap();
        Self {
            region: region.clone(),
            provider: provider.clone(),
            s3: S3Client::new_with(HttpClient::new().unwrap(), provider.clone(), region.clone()),
            cognito: CognitoIdentityClient::new_with(HttpClient::new().unwrap(), provider.clone(), region.clone()),
            config: config.clone(),
            cdn_private_key: RsaPrivateKey::read_pkcs1_pem_file(std::path::Path::new(&config.cdn.private_key_fname)).unwrap(),
        }
    }

    pub fn sign_cloudfront_url(&self, base_url: &str) -> Result<String, SquadOvError> {
        let expires = Utc::now() + Duration::seconds(43200);
        let signature = {
            let policy = format!(
                r#"{{"Statement":[{{"Resource":"{base}","Condition":{{"DateLessThan":{{"AWS:EpochTime":{expires}}}}}}}]}}"#,
                base=base_url,
                expires=expires.timestamp(),
            );

            // Steps are from copying AWS's reference code:
            // https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/CreateSignatureInCSharp.html
            // 1) Create a SHA-1 hash of the actual policy string.
            // 2) Compute an RSA PKCS1v15 Signature (with SHA-1 hasing) using our private key.
            // 3) Encode in URL-safe Base 64
            let policy_hash = {
                let mut hasher = sha1::Sha1::new();
                hasher.update(policy.as_bytes());
                hasher.finalize()
            };
            let policy_signature = self.cdn_private_key.sign(PaddingScheme::PKCS1v15Sign{
                hash: Some(Hash::SHA1)
            }, &policy_hash)?;
            base64::encode_config(&policy_signature, base64::STANDARD_NO_PAD)
                .replace("+", "-")
                .replace("=", "_")
                .replace("/", "~")
        };
        let key_pair_id = self.config.cdn.public_key_id.clone();

        Ok(format!(
            "{base}?Expires={expires}&Signature={signature}&Key-Pair-Id={keypair}",
            base=base_url,
            expires=expires.timestamp(),
            signature=signature,
            keypair=key_pair_id
        ))
    }
}