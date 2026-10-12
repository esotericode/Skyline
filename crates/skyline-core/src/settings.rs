use crate::{
    model::{Catalog, Verification},
    view::Sort,
};
use serde::{Deserialize, Serialize};

/// Array indices change when catalogs are imported; preferences use snapshot identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogChoice {
    pub snapshot_sha256: String,
    pub system: String,
}

impl CatalogChoice {
    pub fn from_catalog(catalog: &Catalog) -> Self {
        Self {
            snapshot_sha256: catalog.provenance.snapshot_sha256.clone(),
            system: catalog.provenance.system.clone(),
        }
    }

    pub fn resolve(&self, catalogs: &[Catalog]) -> Option<usize> {
        catalogs.iter().position(|catalog| {
            catalog.provenance.snapshot_sha256 == self.snapshot_sha256
                && catalog.provenance.system == self.system
        })
    }
}

/// Local preferences only. They never authorize a scan/import or a source mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub dat_path: String,
    pub root_path: String,
    pub exclusions: String,
    pub system: String,
    pub preferred_catalog: Option<CatalogChoice>,
    pub search: String,
    pub status_filter: Option<Verification>,
    pub collection_system: Option<String>,
    pub sort: Sort,
    pub descending: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            dat_path: String::new(),
            root_path: String::new(),
            exclusions: String::new(),
            system: String::new(),
            preferred_catalog: None,
            search: String::new(),
            status_filter: None,
            collection_system: None,
            sort: Sort::Name,
            descending: false,
        }
    }
}
