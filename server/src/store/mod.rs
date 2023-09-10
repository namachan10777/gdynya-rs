use crate::{
    api_schema::{
        CrateName, GetIndexResponse, PostIndexRequest, QueriedPackage, SearchCratesQuery,
    },
    HttpError,
};

pub mod s3;

#[async_trait::async_trait]
pub trait Store {
    async fn health_check(&self) -> Result<(), HttpError>;
    async fn put(&self, index: &PostIndexRequest, body: Vec<u8>) -> Result<(), HttpError>;
    async fn get_index(&self, name: &CrateName) -> Result<Vec<GetIndexResponse>, HttpError>;
    async fn set_yank(
        &self,
        name: &CrateName,
        version: semver::Version,
        yanked: bool,
    ) -> Result<(), HttpError>;
    async fn get_crate(
        &self,
        name: &CrateName,
        version: semver::Version,
    ) -> Result<Vec<u8>, HttpError>;
    async fn get_owners(&self, name: &CrateName) -> Result<Vec<String>, HttpError>;
    async fn add_owner(&self, name: &CrateName, owner: Vec<String>) -> Result<(), HttpError>;
    async fn delete_owner(&self, name: &CrateName, owner: Vec<String>) -> Result<(), HttpError>;
    async fn search(
        &self,
        query: &SearchCratesQuery,
    ) -> Result<(Vec<QueriedPackage>, usize), HttpError>;
}
