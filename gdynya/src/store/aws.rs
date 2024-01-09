use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_s3::primitives::ByteStream;
use axum::http::StatusCode;
use futures_util::future::join_all;
use nom::AsBytes;
use tracing::{debug, info};
use valuable::Valuable;

use crate::{
    api_schema::{
        CrateName, GetIndexResponse, PostIndexRequest, QueriedPackage, SearchCratesQuery,
    },
    HttpError, ToHttpError, ToHttpErrorOption,
};

#[derive(Clone)]
pub struct AwsStore {
    s3: aws_sdk_s3::Client,
    s3_bucket: String,
}

async fn create_s3_client(config: &SdkConfig, endpoint: Option<String>) -> aws_sdk_s3::Client {
    if let Some(endpoint) = endpoint {
        let conf = aws_sdk_s3::config::Builder::from(config)
            .force_path_style(true)
            .endpoint_url(endpoint)
            .build();
        aws_sdk_s3::Client::from_conf(conf)
    } else {
        aws_sdk_s3::Client::new(config)
    }
}

impl AwsStore {
    pub async fn new(s3_bucket: String, s3_endpoint: Option<String>) -> Self {
        info!(s3_bucket, s3_endpoint, "init_s3");
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        AwsStore {
            s3_bucket,
            s3: create_s3_client(&config, s3_endpoint).await,
        }
    }

    async fn list_s3_keys(&self, prefix: &str) -> Result<Vec<String>, HttpError> {
        let response = self
            .s3
            .list_objects_v2()
            .prefix(prefix)
            .bucket(&self.s3_bucket)
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut keys = response
            .contents
            .http_error_with(StatusCode::NOT_FOUND, || "no index")?
            .into_iter()
            .map(|obj| obj.key)
            .map(|key| key.http_error_with(StatusCode::INTERNAL_SERVER_ERROR, || "no key found"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut continuation_token = response.next_continuation_token.clone();
        while let Some(token) = continuation_token.clone() {
            let response = self
                .s3
                .list_objects_v2()
                .bucket(&self.s3_bucket)
                .continuation_token(token)
                .prefix(prefix)
                //.delimiter("/")
                .send()
                .await
                .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
            continuation_token = response.next_continuation_token.clone();
            keys.append(
                &mut response
                    .contents
                    .http_error_with(StatusCode::NOT_FOUND, || "no index")?
                    .into_iter()
                    .map(|obj| obj.key)
                    .map(|key| {
                        key.http_error_with(StatusCode::INTERNAL_SERVER_ERROR, || "no key found")
                    })
                    .collect::<Result<_, _>>()?,
            );
        }
        Ok(keys)
    }

    async fn check_object_existance(&self, key: &str) -> bool {
        self.s3
            .head_object()
            .bucket(&self.s3_bucket)
            .key(key)
            .send()
            .await
            .is_ok()
    }

    async fn get_index_entry(
        &self,
        name: &CrateName,
        version: &semver::Version,
    ) -> Result<GetIndexResponse, HttpError> {
        let index = self
            .s3
            .get_object()
            .bucket(&self.s3_bucket)
            .key(format!("index/{}/{version}", name.normalized))
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?
            .body
            .collect()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?
            .into_bytes();
        serde_json::from_slice(index.as_bytes()).http_error(StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn put_index_entry(&self, index: &GetIndexResponse) -> Result<(), HttpError> {
        self.s3
            .put_object()
            .bucket(&self.s3_bucket)
            .key(format!("index/{}/{}", index.name.normalized, index.vers))
            .content_type("application/json")
            .body(serde_json::to_vec(index).unwrap().into())
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }

    async fn put_crate_archive(
        &self,
        name: &CrateName,
        version: &semver::Version,
        body: impl Into<ByteStream>,
    ) -> Result<(), HttpError> {
        self.s3
            .put_object()
            .bucket(self.s3_bucket.clone())
            .body(body.into())
            .key(format!("crate/{}/{version}", name.normalized))
            .content_type("application/gzip")
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }
}

impl super::Store for AwsStore {
    async fn health_check(&self) -> Result<(), HttpError> {
        self.s3
            .list_buckets()
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }

    async fn set_yank(
        &self,
        name: &CrateName,
        version: semver::Version,
        yanked: bool,
    ) -> Result<(), HttpError> {
        let index = self.get_index_entry(name, &version).await?;
        self.put_index_entry(&GetIndexResponse { yanked, ..index })
            .await?;
        Ok(())
    }

    async fn get_crate(
        &self,
        name: &CrateName,
        version: semver::Version,
    ) -> Result<Vec<u8>, HttpError> {
        let body = self
            .s3
            .get_object()
            .bucket(&self.s3_bucket)
            .key(format!("crate/{}/{version}", name.normalized))
            .send()
            .await
            .http_error(StatusCode::NOT_FOUND)?
            .body
            .collect()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(body.to_vec())
    }

    async fn put(&self, index: &PostIndexRequest, body: Vec<u8>) -> Result<(), HttpError> {
        if self
            .check_object_existance(&format!("index/{}/{}", index.name.normalized, index.vers))
            .await
        {
            return Err(HttpError {
                error_type: StatusCode::BAD_REQUEST,
                message: format!("already exists"),
                verbose_message: format!("{}/{} already exists", index.name.original, index.vers),
                contexts: Vec::new(),
            });
        }
        let index = GetIndexResponse::new(index, &body);
        self.put_index_entry(&index).await?;
        self.put_crate_archive(&index.name, &index.vers, body)
            .await?;
        Ok(())
    }

    async fn get_index(&self, name: &CrateName) -> Result<Vec<GetIndexResponse>, HttpError> {
        let indices = self
            .list_s3_keys(&format!("index/{}/", name.normalized))
            .await?;
        debug!(files = indices.as_value(), "index");
        let indices = indices
            .into_iter()
            .map(|key| {
                key.strip_prefix(&format!("index/{}/", name.normalized))
                    .map(ToString::to_string)
                    .http_error_with(StatusCode::INTERNAL_SERVER_ERROR, || "invalid s3 prefix")
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|s| semver::Version::parse(&s))
            .collect::<Result<Vec<_>, _>>()
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        futures_util::future::join_all(indices.into_iter().map(|version| {
            let fetcher = self.clone();
            async move { fetcher.get_index_entry(name, &version).await }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    }

    async fn get_owners(&self, name: &CrateName) -> Result<Vec<String>, HttpError> {
        self.list_s3_keys(&format!("owner/{}", &name.normalized))
            .await
    }
    async fn delete_owner(&self, name: &CrateName, owner: Vec<String>) -> Result<(), HttpError> {
        let req = owner.into_iter().map(|owner| {
            let deleter = self.clone();
            async move {
                deleter
                    .s3
                    .delete_object()
                    .bucket(self.s3_bucket.clone())
                    .key(format!("owner/{}/{owner}", name.normalized))
                    .send()
                    .await
                    .http_error(StatusCode::INTERNAL_SERVER_ERROR)
            }
        });
        join_all(req)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }
    async fn add_owner(&self, name: &CrateName, owner: Vec<String>) -> Result<(), HttpError> {
        let req = owner.into_iter().map(|owner| {
            let adder = self.clone();
            async move {
                adder
                    .s3
                    .put_object()
                    .bucket(self.s3_bucket.clone())
                    .key(format!("owner/{}/{owner}", name.normalized))
                    .body(Vec::new().into())
                    .send()
                    .await
                    .http_error(StatusCode::INTERNAL_SERVER_ERROR)
            }
        });
        join_all(req)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }
    async fn search(
        &self,
        _query: &SearchCratesQuery,
    ) -> Result<(Vec<QueriedPackage>, usize), HttpError> {
        Err(HttpError {
            error_type: StatusCode::NOT_FOUND,
            message: "search is unsupported".into(),
            verbose_message: "search is unsupported".into(),
            contexts: Default::default(),
        })
    }
}
