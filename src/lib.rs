use serde::{Deserialize, Serialize};

pub mod center;
pub mod commune;
pub mod util;

#[derive(Debug, Serialize, Deserialize)]
pub struct Department {
    code_departement: String,
    nom_departement: String,
    code_region: f64,
    nom_region: String,
}
