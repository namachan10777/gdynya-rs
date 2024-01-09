#![allow(non_upper_case_globals)]
use std::{collections::HashMap, fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use sha2::Sha256;
use valuable::Valuable;

#[derive(Serialize, Deserialize)]
pub enum HttpProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "https")]
    Https,
}

impl Display for HttpProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http => f.write_str("http"),
            Self::Https => f.write_str("https"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtoParseError {
    #[error("unknown protocol {0}")]
    Unknown(String),
}

impl FromStr for HttpProtocol {
    type Err = ProtoParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            _ => Err(ProtoParseError::Unknown(s.to_string())),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Config {
    dl: String,
    api: String,
    #[serde(rename = "auth-required")]
    auth_required: bool,
}

impl Config {
    pub fn new(proto: HttpProtocol, host: &str) -> Self {
        Self {
            dl: format!("{proto}://{host}/api/v1/crates"),
            api: format!("{proto}://{host}"),
            auth_required: true,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    #[serde(rename = "dev")]
    Dev,
    #[serde(rename = "build")]
    Build,
    #[serde(rename = "normal")]
    Normal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostDependency {
    pub name: CrateName,
    pub version_req: semver::VersionReq,
    pub features: Vec<String>,
    pub default_features: bool,
    pub optional: bool,
    pub target: Option<String>,
    pub kind: DependencyKind,
    pub explicit_name_in_toml: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GetDependency {
    pub name: CrateName,
    pub req: semver::VersionReq,
    pub features: Vec<String>,
    pub default_features: bool,
    pub optional: bool,
    pub target: Option<String>,
    pub kind: DependencyKind,
    pub package: Option<String>,
    pub registry: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PostIndexRequest {
    pub name: CrateName,
    pub vers: semver::Version,
    pub deps: Vec<PostDependency>,
    pub features: HashMap<String, Vec<String>>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub documentation: Option<String>,
    pub homepage: Option<String>,
    pub readme: Option<String>,
    pub readme_file: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub license: Option<String>,
    pub license_file: Option<String>,
    pub links: Option<String>,
    pub rust_version: Option<semver::Version>,
    pub badges: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostIndexWarnings {
    pub invalid_categories: Vec<String>,
    pub invalid_badges: Vec<String>,
    pub other: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostIndexResponse {
    pub warnings: PostIndexWarnings,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GetIndexResponse {
    pub name: CrateName,
    pub vers: semver::Version,
    pub deps: Vec<GetDependency>,
    pub features: HashMap<String, Vec<String>>,
    pub links: Option<String>,
    pub cksum: String,
    pub yanked: bool,
    pub v: usize,
    pub rust_version: Option<semver::Version>,
}

#[derive(Debug, thiserror::Error)]
pub enum CrateNameParseError {
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Valuable)]
pub struct CrateName {
    pub original: String,
    pub normalized: String,
}

impl FromStr for CrateName {
    type Err = CrateNameParseError;

    fn from_str(original: &str) -> Result<Self, Self::Err> {
        let mut chars = original.chars();
        let Some(first) = chars.next() else {
            return Err(CrateNameParseError::Empty);
        };
        if !first.is_alphabetic() {
            return Err(CrateNameParseError::FirstChar);
        };
        if chars.any(|c| !c.is_alphanumeric() && !matches!(c, '-' | '_')) {
            return Err(CrateNameParseError::RestChar);
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

impl Serialize for CrateName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.original)
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistryUser {
    pub id: u32,
    pub login: String,
    pub name: String,
}

impl GetIndexResponse {
    pub fn new(index: &PostIndexRequest, body: &[u8]) -> Self {
        use digest::Digest;
        let mut hasher = Sha256::new();
        hasher.update(&body);
        Self {
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
        }
    }
}
