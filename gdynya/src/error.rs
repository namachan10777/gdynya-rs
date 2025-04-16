use std::fmt::{Debug, Display};

use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::{Serialize, ser::SerializeStruct};

#[derive(Debug, Clone, thiserror::Error)]
pub struct HttpError {
    pub error_type: StatusCode,
    pub message: String,
    // maybe include sensitive message
    pub verbose_message: String,
    pub contexts: Vec<String>,
}

pub trait ResponseValidatable: Sized {
    fn validate(self) -> impl Future<Output = Result<Self, HttpError>> + Send;
}

impl ResponseValidatable for reqwest::Response {
    async fn validate(self) -> Result<Self, HttpError> {
        if self.status().is_success() {
            Ok(self)
        } else {
            let status = self.status();
            let message = self
                .text()
                .await
                .http_error(StatusCode::INTERNAL_SERVER_ERROR)?;
            Err(HttpError {
                error_type: status,
                verbose_message: message.clone(),
                message,
                contexts: Default::default(),
            })
        }
    }
}

impl Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl Serialize for HttpError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut serializer = serializer.serialize_struct("HttpError", 3)?;
        serializer.serialize_field("type", &self.error_type.as_u16())?;
        serializer.serialize_field("message", &self.message)?;
        serializer.serialize_field("contexts", &self.contexts)?;
        serializer.end()
    }
}

pub trait ToHttpError {
    type Value;
    fn http_error(self, status: StatusCode) -> Result<Self::Value, HttpError>;
}

pub trait ToHttpErrorOption {
    type Value;
    fn http_error_with<S, F>(
        self,
        status: StatusCode,
        context: F,
    ) -> Result<Self::Value, HttpError>
    where
        F: FnOnce() -> S,
        S: Into<String>;
}

impl<T, E: std::error::Error> ToHttpError for Result<T, E> {
    type Value = T;

    fn http_error(self, status: StatusCode) -> Result<Self::Value, HttpError> {
        self.map_err(|e| HttpError {
            error_type: status,
            message: e.to_string(),
            verbose_message: format!("{:?}", e),
            contexts: Default::default(),
        })
    }
}

impl<T> ToHttpErrorOption for Option<T> {
    type Value = T;
    fn http_error_with<S, F>(self, status: StatusCode, context: F) -> Result<Self::Value, HttpError>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.ok_or_else(|| {
            let message = (context)().into();
            HttpError {
                error_type: status,
                message: message.clone(),
                verbose_message: message,
                contexts: Default::default(),
            }
        })
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> axum::response::Response {
        (self.error_type, Json(self)).into_response()
    }
}
