use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("first charcter must be alphabetic")]
    FirstChar,
    #[error("charcters must be alphanumeric, '_' or '-'")]
    RestChar,
    #[error("empty char disallowed")]
    Empty,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueriedPackage {
    pub name: String,
    pub max_version: semver::Version,
    pub description: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchCratesQuery {
    #[serde(rename = "q")]
    pub q: String,
    #[serde(rename = "per_page")]
    pub per_page: usize,
}

#[derive(Clone)]
pub struct CrateName {
    pub original: String,
    pub normalized: String,
}

impl FromStr for CrateName {
    type Err = Error;

    fn from_str(original: &str) -> Result<Self, Self::Err> {
        let mut chars = original.chars();
        let Some(first) = chars.next() else {
            return Err(Error::Empty);
        };
        if !first.is_alphabetic() {
            return Err(Error::FirstChar);
        };
        if chars.any(|c| !c.is_alphanumeric() && !matches!(c, '-' | '_')) {
            return Err(Error::RestChar);
        }
        Ok(Self {
            original: original.to_string(),
            normalized: original.replace('_', "-"),
        })
    }
}

impl<'de> Deserialize<'de> for CrateName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let original = <&str as Deserialize>::deserialize(deserializer)?;
        let name: CrateName = original.parse().map_err(serde::de::Error::custom)?;
        Ok(name)
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistryUser {
    pub id: u32,
    pub login: String,
    pub name: String,
}
