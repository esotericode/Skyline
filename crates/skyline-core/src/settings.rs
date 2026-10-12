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

/// Bump when a saved field changes meaning, and migrate it in `AppSettings::migrate`.
pub const SETTINGS_REVISION: u32 = 2;

/// Local preferences only. They never authorize a scan/import or a source mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Field-level default: preferences saved before revisions existed read as 0.
    #[serde(default)]
    pub revision: u32,
    pub dat_path: String,
    pub dat_folder: String,
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
            revision: SETTINGS_REVISION,
            dat_path: String::new(),
            dat_folder: String::new(),
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

impl AppSettings {
    /// Revision 0 stored `system` as the required import label, defaulting to
    /// "Game Boy". It is now an optional override of the DAT header name, so a carried
    /// over value would silently relabel every later import.
    pub fn migrate(mut self) -> Self {
        if self.revision < 1 {
            self.system.clear();
        }
        if self.revision < 2 {
            // The folder workflow considers every variant. A stale single-catalog
            // preference must not silently restrict a mixed collection scan.
            self.preferred_catalog = None;
            self.collection_system = self
                .collection_system
                .map(|label| crate::systems::system_family(&label).to_owned());
        }
        self.revision = SETTINGS_REVISION;
        self
    }
}
