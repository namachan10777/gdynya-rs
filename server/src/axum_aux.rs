use axum::{
    extract::FromRequestParts,
    headers::Header,
    http::{request::Parts, HeaderValue},
    response::{IntoResponse, Response},
};

#[derive(Clone)]
pub struct RawAuthorization(String);

impl RawAuthorization {
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl Header for RawAuthorization {
    fn name() -> &'static axum::http::HeaderName {
        &axum::http::header::AUTHORIZATION
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
        Ok(Self(
            value
                .to_str()
                .map_err(|_| axum::headers::Error::invalid())?
                .trim()
                .to_string(),
        ))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        let mut value = HeaderValue::from_str(&self.0).unwrap();
        value.set_sensitive(true);
        values.extend(std::iter::once(value))
    }
}

pub struct OptionalHeader<T>(pub Option<T>);

impl<T: CustomHeader> CustomHeader for OptionalHeader<T> {
    fn name() -> &'static str {
        T::name()
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let values = values.collect::<Vec<_>>();
        if values.is_empty() {
            Ok(Self(None))
        } else {
            Ok(Self(Some(T::decode(&mut values.into_iter())?)))
        }
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        if let Some(header) = &self.0 {
            header.encode(values)
        }
    }
}

pub trait CustomHeader: Sized {
    fn name() -> &'static str;
    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E);
    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>;
}

impl<T: Header> CustomHeader for T {
    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        <T as Header>::decode(values)
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        <T as Header>::encode(self, values)
    }

    fn name() -> &'static str {
        <T as Header>::name().as_str()
    }
}

pub struct CustomTypedHeader<T>(pub T);

enum CustomTypedHeaderRejectionReason {
    Missing,
    Error(axum::headers::Error),
}

pub struct CustomTypedHeaderRejection {
    name: &'static str,
    reason: CustomTypedHeaderRejectionReason,
}

impl IntoResponse for CustomTypedHeaderRejection {
    fn into_response(self) -> Response {
        (axum::http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}

impl std::fmt::Display for CustomTypedHeaderRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.reason {
            CustomTypedHeaderRejectionReason::Missing => {
                write!(f, "Header of type `{}` was missing", self.name)
            }
            CustomTypedHeaderRejectionReason::Error(err) => {
                write!(f, "{} ({})", err, self.name)
            }
        }
    }
}

#[async_trait::async_trait]
impl<T, S> FromRequestParts<S> for CustomTypedHeader<T>
where
    T: CustomHeader,
    S: Send + Sync,
{
    type Rejection = CustomTypedHeaderRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let mut values = parts.headers.get_all(T::name()).iter();
        let is_missing = values.size_hint() == (0, Some(0));
        T::decode(&mut values)
            .map(Self)
            .map_err(|err| CustomTypedHeaderRejection {
                name: T::name(),
                reason: if is_missing {
                    // Report a more precise rejection for the missing header case.
                    CustomTypedHeaderRejectionReason::Missing
                } else {
                    CustomTypedHeaderRejectionReason::Error(err)
                },
            })
    }
}

pub struct XForwardedHost(pub String);

impl CustomHeader for XForwardedHost {
    fn name() -> &'static str {
        "X-Forwarded-Host"
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values
            .next()
            .ok_or_else(axum::headers::Error::invalid)?
            .to_str()
            .map_err(|_| axum::headers::Error::invalid())?;
        Ok(Self(value.into()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend(std::iter::once(HeaderValue::from_str(&self.0).unwrap()))
    }
}

pub struct XForwardedProto(pub String);

impl CustomHeader for XForwardedProto {
    fn name() -> &'static str {
        "X-Forwarded-Proto"
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values
            .next()
            .ok_or_else(axum::headers::Error::invalid)?
            .to_str()
            .map_err(|_| axum::headers::Error::invalid())?;
        Ok(Self(value.into()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend(std::iter::once(HeaderValue::from_str(&self.0).unwrap()))
    }
}
