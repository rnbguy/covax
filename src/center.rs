use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use chrono::{DateTime, FixedOffset};

use cli_table::Table;

#[derive(Debug, Serialize, Deserialize)]
pub struct Location {
    longitude: f64,
    latitude: f64,
    city: Option<String>,
    cp: Option<String>,
}

type Date = Option<DateTime<FixedOffset>>;

mod my_date_format {
    use chrono::{DateTime, FixedOffset, NaiveDateTime};
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(
        date: &Option<DateTime<FixedOffset>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(date) = date {
            serializer.serialize_str(date.format("%+").to_string().as_str())
        } else {
            serializer.serialize_none()
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<FixedOffset>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = Option::<String>::deserialize(deserializer)?;
        s.map(|s| {
            DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f%:z")
                .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f%z"))
                .or_else(|_| {
                    NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f")
                        .map(|ndt| DateTime::from_utc(ndt, FixedOffset::east(2 * 3600)))
                })
                .map_err(serde::de::Error::custom)
        })
        .transpose()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestCount {
    slots: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppointmentSchedule {
    name: String,
    #[serde(with = "my_date_format")]
    from: Date,
    #[serde(with = "my_date_format")]
    to: Date,
    total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    address: String,
    business_hours: Option<HashMap<String, Option<String>>>,
    phone_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Center {
    departement: String,
    nom: String,
    url: String,
    location: Option<Location>,
    metadata: Metadata,
    #[serde(with = "my_date_format")]
    prochain_rdv: Date,
    plateforme: Option<String>,
    #[serde(rename = "type")]
    _type: String,
    appointment_count: usize,
    internal_id: Option<String>,
    vaccine_type: Option<Vec<String>>,
    appointment_by_phone_only: bool,
    erreur: Option<String>,
    #[serde(with = "my_date_format")]
    last_scan_with_availabilities: Date,
    request_counts: Option<RequestCount>,
    appointment_schedules: Option<Vec<AppointmentSchedule>>,
    gid: String,
}

#[derive(Table, PartialOrd, PartialEq)]
pub struct CenterInfo {
    #[table(title = "Km")]
    pub distance: f64,
    #[table(title = "Slots")]
    pub n_slot: usize,
    #[table(title = "Next RDV")]
    date: String,
    #[table(title = "Address")]
    address: String,
    #[table(title = "URL")]
    url: String,
}

impl Center {
    pub fn has_chronodose(&self) -> bool {
        self.appointment_schedules
            .as_ref()
            .and_then(|x| x.iter().find(|x| x.name == "chronodose"))
            .map(|x| x.total > 0)
            == Some(true)
    }

    pub fn has_vaccine(&self, pat: &str) -> bool {
        self.vaccine_type
            .as_ref()
            .map(|vc| vc.iter().any(|x| x.to_ascii_lowercase().contains(pat)))
            == Some(true)
    }

    pub async fn info(
        self,
        latitude: f64,
        longitude: f64,
        distance_limit: f64,
    ) -> anyhow::Result<CenterInfo> {
        let distance = self
            .location
            .as_ref()
            .map(|location| {
                crate::util::lat_long_to_km(
                    latitude,
                    longitude,
                    location.latitude,
                    location.longitude,
                )
            })
            .unwrap_or(f64::MAX);

        if distance <= distance_limit {
            Ok(CenterInfo {
                distance: (distance * 100.).round() / 100.,
                n_slot: if self.url.contains("doctolib") {
                    crate::service::doctolib::process_doctolib_center(&self.url, 0).await?
                } else {
                    self.appointment_schedules
                        .as_ref()
                        .and_then(|x| x.iter().find(|x| x.name == "chronodose"))
                        .map(|x| x.total)
                        .unwrap_or_default()
                },
                date: self
                    .prochain_rdv
                    .map(|x| x.to_rfc2822())
                    .unwrap_or_default(),
                address: self.metadata.address.to_owned(),
                url: self.url.to_owned(),
            })
        } else {
            Err(anyhow::Error::msg("distance filter"))
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CentersInDepartment {
    version: usize,
    last_updated: String,
    last_scrap: Vec<String>,
    pub centres_disponibles: Vec<Center>,
    centres_indisponibles: Vec<Center>,
}
