use axum::http::StatusCode;
use digest::Digest;
use futures_util::future::join_all;
use nom::AsBytes;
use sha2::Sha256;
use tracing::{debug, info};
use valuable::Valuable;

use crate::{
    api_schema::{
        CrateName, GetDependency, GetIndexResponse, PostIndexRequest, QueriedPackage,
        SearchCratesQuery,
    },
    HttpError, ToHttpError, ToHttpErrorOption,
};

#[derive(Clone)]
pub struct S3Store {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Store {
    pub async fn new(bucket: String, endpoint: Option<String>) -> Self {
        info!(bucket, endpoint, "init_s3");
        if let Some(endpoint) = endpoint {
            let config = aws_config::load_from_env().await;
            let conf = aws_sdk_s3::config::Builder::from(&config)
                .force_path_style(true)
                .endpoint_url(endpoint)
                .build();
            let client = aws_sdk_s3::Client::from_conf(conf);
            S3Store { client, bucket }
        } else {
            let config = aws_config::load_from_env().await;
            let client = aws_sdk_s3::Client::new(&config);
            S3Store { client, bucket }
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, HttpError> {
        let response = self
            .client
            .list_objects_v2()
            //.delimiter("/")
            .prefix(prefix)
            .bucket(&self.bucket)
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
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
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

    async fn get_index_entry(
        &self,
        name: &CrateName,
        version: &semver::Version,
    ) -> Result<GetIndexResponse, HttpError> {
        let index = self
            .client
            .get_object()
            .bucket(&self.bucket)
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
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(format!("index/{}/{}", index.name.normalized, index.vers))
            .content_type("application/json")
            .body(serde_json::to_vec(index).unwrap().into())
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }
}

impl super::Store for S3Store {
    async fn health_check(&self) -> Result<(), HttpError> {
        self.client
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
            .client
            .get_object()
            .bucket(&self.bucket)
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
        let ver = index.vers.clone();
        let name = index.name.clone();
        let mut hasher = Sha256::new();
        hasher.update(&body);
        let index = GetIndexResponse {
            name: index.name.clone(),
            vers: index.vers.clone(),
            deps: index
                .deps
                .iter()
                .map(|dep| GetDependency {
                    name: dep.name.clone(),
                    req: dep.version_req.clone(),
                    features: dep.features.clone(),
                    default_features: dep.default_features,
                    optional: dep.optional,
                    target: dep.target.clone(),
                    kind: dep.kind,
                    package: None,
                    registry: None,
                })
                .collect(),
            features: index.features.clone(),
            links: index.links.clone(),
            yanked: false,
            cksum: hex::encode(hasher.finalize()),
            v: 2,
            rust_version: index.rust_version.clone(),
        };
        self.put_index_entry(&index).await?;
        self.client
            .put_object()
            .bucket(self.bucket.clone())
            .body(body.into())
            .key(format!("crate/{}/{ver}", name.normalized))
            .content_type("application/gzip")
            .send()
            .await
            .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }

    async fn get_index(&self, name: &CrateName) -> Result<Vec<GetIndexResponse>, HttpError> {
        let indices = self.list(&format!("index/{}/", name.normalized)).await?;
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
        self.list(&format!("owner/{}", &name.normalized)).await
    }
    async fn delete_owner(&self, name: &CrateName, owner: Vec<String>) -> Result<(), HttpError> {
        let req = owner.into_iter().map(|owner| {
            let deleter = self.clone();
            async move {
                deleter
                    .client
                    .delete_object()
                    .bucket(self.bucket.clone())
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
                    .client
                    .put_object()
                    .bucket(self.bucket.clone())
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
