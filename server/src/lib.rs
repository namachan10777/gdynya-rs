use std::fmt::{Debug, Display};

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{ser::SerializeStruct, Serialize};

pub mod auth;
pub mod index_schema;
pub mod store;
pub mod types;

#[derive(Debug, Clone, thiserror::Error)]
pub struct HttpError {
    pub error_type: StatusCode,
    pub message: String,
    pub verbose_message: String,
    pub contexts: Vec<String>,
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
        let mut serializer = serializer.serialize_struct("HttpError", 2)?;
        serializer.serialize_field("type", &self.error_type.as_u16())?;
        serializer.serialize_field("message", &self.message)?;
        serializer.serialize_field("verbose", &self.verbose_message)?;
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
