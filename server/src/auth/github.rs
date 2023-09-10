use std::{collections::HashMap, sync::Arc};

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    api_schema::{CrateName, RegistryUser},
    axum_aux::RawAuthorization,
    HttpError, ResponseValidatable, ToHttpError, ToHttpErrorOption,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Rule {
    #[serde(rename = "is")]
    Is { user: String },
    #[serde(rename = "in_orgs")]
    InOrgs { org: String },
}

mod permission_test {
    use serde::Deserialize;

    use crate::ResponseValidatable;

    #[derive(Deserialize)]
    struct GhUserResponse {
        login: String,
    }
    async fn get_user(token: &str) -> anyhow::Result<String> {
        let user = reqwest::Client::new()
            .get("https://api.github.com/user")
            .bearer_auth(token)
            .header("user-agent", "prates-io")
            .header("x-github-api-version", "2022-11-28")
            .header("accept", "application/vnd.github+json")
            .send()
            .await?
            .validate()
            .await?
            .json::<GhUserResponse>()
            .await?;
        Ok(user.login)
    }

    pub async fn in_orgs(token: &str, org: &str) -> anyhow::Result<bool> {
        let members = reqwest::Client::new()
            .get(format!("https://api.github.com/orgs/{org}/members"))
            .bearer_auth(token)
            .header("user-agent", "prates-io")
            .header("x-github-api-version", "2022-11-28")
            .header("accept", "application/vnd.github+json")
            .send()
            .await?
            .validate()
            .await?
            .json::<Vec<GhUserResponse>>()
            .await?;
        let me = get_user(token).await?;
        Ok(members.iter().any(|member| member.login == me))
    }

    pub async fn is(token: &str, user: &str) -> anyhow::Result<bool> {
        let me = get_user(token).await?;
        Ok(me == user)
    }
}

impl Rule {
    async fn test(&self, token: &str) -> anyhow::Result<bool> {
        match self {
            Self::InOrgs { org } => permission_test::in_orgs(token, org).await,
            Self::Is { user } => permission_test::is(token, user).await,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrateRule {
    pub write: Rule,
    pub read: Rule,
}

pub type AuthRules = HashMap<String, CrateRule>;

#[derive(Hash, PartialEq, Eq, Clone)]
struct CacheKey {
    crate_name: String,
    token: String,
}

#[derive(Clone)]
pub struct GitHubAuth {
    auth_rules: AuthRules,
    read_cache: Arc<moka::future::Cache<CacheKey, bool>>,
    write_cache: Arc<moka::future::Cache<CacheKey, bool>>,
}

impl GitHubAuth {
    pub fn new_from_config(auth_rules: AuthRules) -> Self {
        Self {
            auth_rules,
            read_cache: Arc::new(moka::future::Cache::new(1024)),
            write_cache: Arc::new(moka::future::Cache::new(1024)),
        }
    }

    async fn test_read(&self, key: &CacheKey) -> Result<bool, HttpError> {
        let rule = self
            .auth_rules
            .get(&key.crate_name)
            .http_error_with(StatusCode::FORBIDDEN, || "forbidden")?;
        Ok(rule.read.test(&key.token).await.is_ok())
    }

    async fn test_write(&self, key: &CacheKey) -> Result<bool, HttpError> {
        let rule = self
            .auth_rules
            .get(&key.crate_name)
            .http_error_with(StatusCode::FORBIDDEN, || "forbidden")?;
        Ok(rule.write.test(&key.token).await.is_ok())
    }
}

#[derive(Deserialize)]
struct GhUserResponse {
    id: u32,
    login: String,
    name: String,
}

#[async_trait::async_trait]
impl super::Auth for GitHubAuth {
    async fn readable(&self, token: &RawAuthorization, name: &CrateName) -> Result<(), HttpError> {
        let key = CacheKey {
            crate_name: name.normalized.clone(),
            token: token.value().to_string(),
        };
        let result = if let Some(result) = self.read_cache.get(&key) {
            result
        } else {
            let result = self.test_read(&key).await.unwrap_or(false);
            self.read_cache.insert(key.clone(), result).await;
            let read_cache = self.read_cache.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                read_cache.invalidate(&key).await
            });
            result
        };
        if result {
            Ok(())
        } else {
            Err(HttpError {
                error_type: StatusCode::FORBIDDEN,
                message: "forbidden".to_string(),
                verbose_message: "forbidden".to_string(),
                contexts: Default::default(),
            })
        }
    }
    async fn writable(&self, token: &RawAuthorization, name: &CrateName) -> Result<(), HttpError> {
        let key = CacheKey {
            crate_name: name.normalized.clone(),
            token: token.value().to_string(),
        };
        let result = if let Some(result) = self.write_cache.get(&key) {
            result
        } else {
            let result = self.test_write(&key).await.unwrap_or(false);
            self.write_cache.insert(key.clone(), result).await;
            let write_cache = self.write_cache.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                write_cache.invalidate(&key).await
            });
            result
        };
        if result {
            Ok(())
        } else {
            Err(HttpError {
                error_type: StatusCode::FORBIDDEN,
                message: "forbidden".to_string(),
                verbose_message: "forbidden".to_string(),
                contexts: Default::default(),
            })
        }
    }
    async fn as_registry_user(
        &self,
        token: &RawAuthorization,
        user: &str,
    ) -> Result<RegistryUser, HttpError> {
        let user = reqwest::Client::new()
            .get(format!("https://api.github.com/users/{user}"))
            .bearer_auth(token.value())
            .header("user-agent", "prates-io")
            .header("x-github-api-version", "2022-11-28")
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .http_error(StatusCode::FORBIDDEN)?
            .validate()
            .await?
            .json::<GhUserResponse>()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(RegistryUser {
            id: user.id,
            login: user.login,
            name: user.name,
        })
    }
}
