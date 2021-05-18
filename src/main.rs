use anyhow::Result;
use futures::future::join_all;
use log::info;

use covax::center::{CenterInfo, CentersInDepartment};

use cli_table::{print_stdout, WithTitle};

// static COVIDTRACKER: &str = "https://vitemadose.covidtracker.fr/";
static GITLAB: &str = "https://vitemadose.gitlab.io/vitemadose/";

// lazy_static::lazy_static! {
//     static ref DEPTS: Vec<Department> = serde_json::from_str(include_str!("data/departements.json")).unwrap();
// }

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // departements around Paris
    let depts = vec![75, 77, 78, 91, 92, 93, 94, 95];

    // musee du Louvre geo-location
    let (lat, long) = (48.864824, 2.334595);

    let departments: Vec<anyhow::Result<CentersInDepartment>> = join_all(
        // includes all main-land french departements
        // (1..=95)
        depts
            .into_iter()
            .map(|d: usize| async move {
                Ok(reqwest::get(&format!("{}{:02}.json", GITLAB, d))
                    .await?
                    .json()
                    .await?)
            })
            .collect::<Vec<_>>(),
    )
    .await;

    info!(
        "Parsed data of {} department(s).",
        departments.iter().filter(|x| x.is_ok()).count()
    );

    let data: Vec<anyhow::Result<CenterInfo>> = join_all(
        departments
            .into_iter()
            .filter_map(|x| x.ok())
            .map(|x| x.centres_disponibles)
            .flatten()
            // .filter(|c| c.has_chronodose() && c.has_vaccine("pfizer"))
            .map(|x| x.info(lat, long, 50000.))
            .collect::<Vec<_>>(),
    )
    .await;

    let mut data: Vec<_> = data
        .into_iter()
        .flatten()
        .filter(|x| x.n_slot > 0)
        .collect();

    data.sort_by(|a, b| a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal));

    print_stdout(data.with_title())?;

    Ok(())
}
