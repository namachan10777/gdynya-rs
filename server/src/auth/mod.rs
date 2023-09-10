use crate::{
    types::{CrateName, RegistryUser},
    HttpError,
};

pub mod github;

#[async_trait::async_trait]
pub trait Auth {
    async fn readable(&self, token: &str, name: &CrateName) -> Result<(), HttpError>;
    async fn writable(&self, token: &str, name: &CrateName) -> Result<(), HttpError>;
    async fn as_registry_user(&self, token: &str, user: &str) -> Result<RegistryUser, HttpError>;
}
