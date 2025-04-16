use futures_util::Future;

use crate::{
    HttpError,
    api_schema::{CrateName, RegistryUser},
    axum_aux::RawAuthorization,
};

pub mod github;

pub trait Auth {
    fn readable(
        &self,
        token: &RawAuthorization,
        name: &CrateName,
    ) -> impl Future<Output = Result<(), HttpError>>;
    fn writable(
        &self,
        token: &RawAuthorization,
        name: &CrateName,
    ) -> impl Future<Output = Result<(), HttpError>>;
    fn as_registry_user(
        &self,
        token: &RawAuthorization,
        user: &str,
    ) -> impl Future<Output = Result<RegistryUser, HttpError>>;
}
