use std::collections::HashMap;
use crate::database::schema::crates;
use serde::{Deserialize, Serialize};

#[derive(Queryable, Insertable, Serialize, Deserialize, Debug, Default)]
#[table_name = "crates"]
pub struct Crates {
    #[serde(skip_serializing)]
    pub id: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub updated_at: String,
    #[serde(skip_serializing)]
    pub versions: Option<String>,
    #[serde(skip_serializing)]
    pub keywords: Option<String>,
    #[serde(skip_serializing)]
    pub categories: Option<String>,
    #[serde(skip_serializing)]
    pub created_at: String,
    #[serde(skip_serializing)]
    pub downloads: i32,
    #[serde(skip_serializing)]
    pub recent_downloads: i32,
    pub max_version: String,
    #[serde(skip_serializing)]
    pub newest_version: String,
    #[serde(skip_serializing)]
    pub max_stable_version: Option<String>,
    pub description: Option<String>,
    #[serde(skip_serializing)]
    pub homepage: Option<String>,
    #[serde(skip_serializing)]
    pub documentation: Option<String>,
    #[serde(skip_serializing)]
    pub repository: Option<String>,

    #[serde(skip)]
    pub owners: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
/// crate metadata received from cargo publish
pub struct CrateInfo {
    /// The name of the package
    pub name: String,
    /// The version of the package being published
    pub vers: String,
    /// Array of direct dependenices of the package
    pub deps: Vec<Dependency>,
    /// Set of features defined for the package.
    /// Each feature maps to an array of features or dependencies it enables.
    /// Cargo does not impose limitations on feature names, but crates.io
    /// requires alphanumeric ASCII, `_` or `-` characters.
    pub features: HashMap<String, Vec<String>>,
    /// List of strings of the authors.
    /// May be empty. crates.io requires at least one entry.
    pub authors: Vec<String>,
    /// Description field from the manifest.
    /// May be null. crates.io requires at least some content.
    pub description: Option<String>,
    /// String of the URL to the website for this package's documentation.
    /// May be null.
    pub documentation: Option<String>,
    /// String of the URL to the website for this package's home page.
    /// May be null.
    pub homepage: Option<String>,
    /// String of the content of the README file.
    /// May be null
    pub readme: Option<String>,
    /// String of a relative path to a README file in the crate.
    /// May be null
    pub readme_file: Option<String>,
    /// Array of strings of keywords for the package
    pub keywords: Vec<String>,
    /// Array of strings of categories for the package
    pub categories: Vec<String>,
    /// String fo the license for the package
    /// May be null. crates.io requires either `license` or `license_file` to be set.
    pub license: Option<String>,
    /// String of relative path to a license file in the crate.
    /// May be null
    pub license_file: Option<String>,
    /// String of the URL to the website for the source repository of this package.
    /// May be null
    pub repository: Option<String>,
    /// Optional object of "status" badges. Each value is an object of
    /// arbitrary string to string mappings.
    /// crates.io has special interpretation of the format of the badges.
    pub badges: HashMap<String, HashMap<String, String>>,
    /// The `links` string value from the package's manifest, or null if not
    /// specified. This field is optional and defaults to null.
    pub links: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Dependency {
    /// Name of dependency
    /// If the dependency is renamed from the original package name,
    /// this is the original name. The new package name is stored in
    /// the `explicit_name_in_toml` field
    name: String,
    #[serde(alias = "req")]
    /// The semver requirement for the dependency
    version_req: String,
    /// Array of features (as string) enabled for this dependency
    features: Vec<String>,
    /// Boolean of whether or not this is an optional dependency
    optional: bool,
    /// Boolean of whether or not default features are enabled
    default_features: bool,
    /// The target platform for the dependency
    /// null if not a target dependency
    /// Otherwise, a string such as "cfg(windows)"
    target: Option<String>,
    /// The dependency kind.
    /// "dev", "buiild", or "normal"
    kind: Option<String>,
    /// The URL of the index of the registry where this dependency is
    /// from as string. If not specified or null, it is assumed the
    /// dependency is in the current registry
    registry: Option<String>,
    /// If the dependency is renamed, this is a string of the new
    /// package name. If not specified or null, this dependency is not
    /// renamed.
    explicit_name_in_toml: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
/// metadata in index repository
pub struct IndexMetadata {
    pub name: String,
    pub vers: String,
    pub deps: Vec<Dependency>,
    pub cksum: String,
    pub features: HashMap<String, Vec<String>>,
    pub yanked: bool,
    pub links: Option<String>,
}
