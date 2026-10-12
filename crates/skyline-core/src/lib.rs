pub mod catalog;
pub mod catalog_folder;
pub mod formats;
pub mod hash;
pub mod matching;
pub mod model;
mod reference;
pub mod scan;
pub mod settings;
pub mod store;
pub mod systems;
pub mod view;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
