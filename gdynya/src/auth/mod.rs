use crate::{
    api_schema::{CrateName, RegistryUser},
    axum_aux::RawAuthorization,
    HttpError,
};

pub mod github;

#[async_trait::async_trait]
pub trait Auth {
    async fn readable(&self, token: &RawAuthorization, name: &CrateName) -> Result<(), HttpError>;
    async fn writable(&self, token: &RawAuthorization, name: &CrateName) -> Result<(), HttpError>;
    async fn as_registry_user(
        &self,
        token: &RawAuthorization,
        user: &str,
    ) -> Result<RegistryUser, HttpError>;
}
