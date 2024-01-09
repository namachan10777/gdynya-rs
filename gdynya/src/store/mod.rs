use futures_util::Future;

use crate::{
    api_schema::{
        CrateName, GetIndexResponse, PostIndexRequest, QueriedPackage, SearchCratesQuery,
    },
    HttpError,
};

pub mod s3;

pub trait Store {
    fn health_check(&self) -> impl Future<Output = Result<(), HttpError>> + Send;
    fn put(
        &self,
        index: &PostIndexRequest,
        body: Vec<u8>,
    ) -> impl Future<Output = Result<(), HttpError>> + Send;
    fn get_index(
        &self,
        name: &CrateName,
    ) -> impl Future<Output = Result<Vec<GetIndexResponse>, HttpError>> + Send;
    fn set_yank(
        &self,
        name: &CrateName,
        version: semver::Version,
        yanked: bool,
    ) -> impl Future<Output = Result<(), HttpError>> + Send;
    fn get_crate(
        &self,
        name: &CrateName,
        version: semver::Version,
    ) -> impl Future<Output = Result<Vec<u8>, HttpError>> + Send;
    fn get_owners(
        &self,
        name: &CrateName,
    ) -> impl Future<Output = Result<Vec<String>, HttpError>> + Send;
    fn add_owner(
        &self,
        name: &CrateName,
        owner: Vec<String>,
    ) -> impl Future<Output = Result<(), HttpError>> + Send;
    fn delete_owner(
        &self,
        name: &CrateName,
        owner: Vec<String>,
    ) -> impl Future<Output = Result<(), HttpError>> + Send;
    fn search(
        &self,
        query: &SearchCratesQuery,
    ) -> impl Future<Output = Result<(Vec<QueriedPackage>, usize), HttpError>> + Send;
}
