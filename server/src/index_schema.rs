use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
    pub name: String,
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
    pub name: String,
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
    pub name: String,
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
    invalid_categories: Vec<String>,
    invalid_badges: Vec<String>,
    other: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostIndexResponse {
    warnings: PostIndexWarnings,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GetIndexResponse {
    pub name: String,
    pub vers: semver::Version,
    pub deps: Vec<GetDependency>,
    pub features: HashMap<String, Vec<String>>,
    pub links: Option<String>,
    pub cksum: String,
    pub yanked: bool,
    pub v: usize,
    pub rust_version: Option<semver::Version>,
}
