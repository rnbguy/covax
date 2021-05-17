use serde::{Deserialize, Serialize};

mod my_geolocation_format {
    use super::GeoLocation;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(geolocation: &Option<GeoLocation>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(geolocation) = geolocation {
            serializer.serialize_bytes(
                format!("{},{}", geolocation.latitude, geolocation.longitude).as_bytes(),
            )
        } else {
            serializer.serialize_none()
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<GeoLocation>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = Option::<String>::deserialize(deserializer)?;
        s.map(|s| {
            if let Some((lat, long)) = s.split_once(',') {
                let latitude = lat.parse().map_err(serde::de::Error::custom)?;
                let longitude = long.parse().map_err(serde::de::Error::custom)?;
                Ok(GeoLocation {
                    longitude,
                    latitude,
                })
            } else {
                Err(serde::de::Error::custom("couldn't split"))
            }
        })
        .transpose()
    }
}

#[derive(Debug)]
pub struct GeoLocation {
    longitude: f64,
    latitude: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Commune {
    c: String,
    z: String,
    n: String,
    d: Option<String>,
    #[serde(default)]
    #[serde(with = "my_geolocation_format")]
    g: Option<GeoLocation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommuneResponse {
    query: String,
    communes: Vec<Commune>,
}
