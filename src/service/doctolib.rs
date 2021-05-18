use anyhow::Result;
use chrono::Utc;
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use futures::{
    future::join_all,
    stream::{self, StreamExt},
};
use log::info;
use rand::Rng;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone)]
struct Center {
    agenda_ids: String,
    practice_ids: String,
    visit_motive_ids: String,
}

fn gen_random_limit() -> String {
    rand::thread_rng().gen_range::<usize, _>(4..=4).to_string()
}

fn motive_filter(name: &str) -> bool {
    let name = name.replace("19", "").to_lowercase();
    name.contains('1') && name.contains("pfizer")
}

impl Center {
    fn new(agenda_ids: String, practice_ids: String, visit_motive_ids: String) -> Self {
        Center {
            agenda_ids,
            practice_ids,
            visit_motive_ids,
        }
    }

    async fn check_availablity(&self, start_date: &str) -> Result<Value> {
        let client = reqwest::Client::builder().build()?;

        let limit = gen_random_limit();
        let query_params = vec![
            ("start_date", start_date),
            ("visit_motive_ids", self.visit_motive_ids.as_str()),
            ("agenda_ids", self.agenda_ids.as_str()),
            ("insurance_sector", "public"),
            ("practice_ids", self.practice_ids.as_str()),
            ("destroy_temporary", "true"),
            ("limit", limit.as_str()),
        ];

        let response = client
            .get("https://www.doctolib.fr/availabilities.json")
            .query(&query_params)
            .send()
            .await?;

        Ok(response.json().await?)
    }

    async fn check_appointment(&self, slots: &[&str]) -> Result<Vec<Value>> {
        // be careful when you check an appointment, it actually claims the slot for a cookie
        // without cookie-store, it will claim all the slots and make them unavailable temporarily
        // even with cookie, it will claim at least one slot
        // to unclaim even that one, claim any other unavailable slots (such as a time one hour ago)

        let client = reqwest::Client::builder()
            .cookie_store(true) // cookie-store is on
            .build()?;

        let results = stream::iter(slots)
            .then(|slot| self._check_appointment(slot, &client))
            .collect::<Vec<anyhow::Result<Value>>>()
            .await;

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let fake_slot = DateTime::parse_from_str(slots[0], "%Y-%m-%dT%H:%M:%S%.f%:z")
            .or_else(|_| DateTime::parse_from_str(slots[0], "%Y-%m-%dT%H:%M:%S%.f%z"))
            .or_else(|_| {
                NaiveDateTime::parse_from_str(slots[0], "%Y-%m-%dT%H:%M:%S%.f")
                    .map(|ndt| DateTime::from_utc(ndt, FixedOffset::east(2 * 3600)))
            })
            .ok()
            .and_then(|x| x.checked_add_signed(chrono::Duration::days(10)))
            .unwrap();

        // let fake_slot = Utc::now();

        // reset unclaimed
        info!(
            "should be slot unavailable (true): {}",
            self._check_appointment(&fake_slot.to_rfc3339(), &client)
                .await?
                .pointer("/error")
                .is_some()
        );

        // let limit = gen_random_limit();
        // let query_params = vec![
        //     ("start_date", &slots[0][0..10]),
        //     ("visit_motive_ids", self.visit_motive_ids.as_str()),
        //     ("agenda_ids", self.agenda_ids.as_str()),
        //     ("insurance_sector", "public"),
        //     ("practice_ids", self.practice_ids.as_str()),
        //     ("destroy_temporary", "true"),
        //     ("limit", limit.as_str()),
        // ];

        // let response = client
        //     .get("https://www.doctolib.fr/availabilities.json")
        //     .query(&query_params)
        //     .send()
        //     .await?;

        // info!(
        //     "reset avl {:?}",
        //     serde_json::to_string_pretty(&response.json().await?)
        // );

        Ok(results.into_iter().filter_map(|x| x.ok()).collect())
    }

    async fn _check_appointment(&self, slot: &str, client: &Client) -> Result<Value> {
        let post_data = json!({
            "agenda_ids": self.agenda_ids,
            "practice_ids": [self.practice_ids],
            "appointment": {
                "start_date": slot,
                "visit_motive_ids": self.visit_motive_ids
            }
        });

        let request = client
            .post("https://www.doctolib.fr/appointments.json")
            .json(&post_data);

        // info!("{:?}", request);
        // info!("{}", serde_json::to_string_pretty(&post_data).unwrap());

        let response = request.send().await?;

        Ok(response.json().await?)
    }

    async fn _check_second_availablity(
        &self,
        second_start_date: &str,
        first_slot: &str,
    ) -> Result<Value> {
        let client = reqwest::Client::builder().build()?;

        let limit = gen_random_limit();
        let query_params = vec![
            ("start_date", second_start_date),
            ("visit_motive_ids", self.visit_motive_ids.as_str()),
            ("agenda_ids", self.agenda_ids.as_str()),
            ("first_slot", first_slot),
            ("insurance_sector", "public"),
            ("practice_ids", self.practice_ids.as_str()),
            ("destroy_temporary", "true"),
            ("limit", limit.as_str()),
        ];

        let response = client
            .get("https://www.doctolib.fr/second_shot_availabilities.json")
            .query(&query_params)
            .send()
            .await?;

        info!("{}", response.url().to_string());

        Ok(response.json().await?)
    }
}

pub async fn process_doctolib_center(center_url: &str, days: usize) -> anyhow::Result<usize> {
    let url = reqwest::Url::parse(center_url)?;
    let path_segs = url.path_segments().unwrap().collect::<Vec<_>>();
    let center_id = path_segs.last().unwrap();

    let practice_id = url
        .query_pairs()
        .find(|(x, _)| x == "pid")
        .map(|(_, y)| y.split_once('-').unwrap().1.to_string())
        .unwrap();

    info!("Found center id: {}", center_id);
    info!("Found practice id: {}", practice_id);

    let center_data: Value = reqwest::get(format!(
        "https://www.doctolib.fr/booking/{}.json",
        center_id
    ))
    .await?
    .json()
    .await?;

    let visit_motive_ids: Vec<_> = center_data
        .pointer("/data/visit_motives")
        .and_then(|x| x.as_array())
        .unwrap()
        .iter()
        .filter(|x| motive_filter(x.pointer("/name").and_then(|x| x.as_str()).unwrap()))
        .inspect(|x| {
            info!(
                "{:?}: {:?}",
                x.pointer("/id")
                    .and_then(|x| serde_json::to_string_pretty(x).ok())
                    .unwrap(),
                x.pointer("/name")
                    .and_then(|x| serde_json::to_string_pretty(x).ok())
                    .unwrap()
            )
        })
        .filter_map(|x| x.pointer("/id").and_then(|x| x.as_u64()))
        .collect();

    let mut agendas: Vec<_> = center_data
        .pointer("/data/agendas")
        .and_then(|x| x.as_array())
        .unwrap()
        .iter()
        // .filter(|x| x["booking_disabled"].as_bool().unwrap())
        .map(|x| {
            (
                x["id"].as_u64().unwrap(),
                x["visit_motive_ids_by_practice_id"].as_object().unwrap(),
            )
        })
        .map(|(id, map)| {
            map.into_iter()
                .filter(|(k, _)| k == &&practice_id)
                .map(move |(k, v)| {
                    let a: Vec<u64> = v
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_u64().unwrap())
                        .collect();
                    // a.retain(|x| (&visit_motive_ids).contains(&x));
                    (id, k, a)
                })
        })
        .flatten()
        .collect();

    agendas
        .iter_mut()
        .for_each(|(_, _, m)| m.retain(|x| (&visit_motive_ids).contains(&x)));

    agendas.retain(|(_, _, m)| !m.is_empty());

    if agendas.is_empty() {
        return Ok(0);
    }

    let agenda_ids: HashSet<_> = agendas.iter().map(|(i, _, _)| i).collect();

    let practice_ids: HashSet<_> = agendas.iter().map(|(_, k, _)| k).collect();

    let visit_motive_ids: HashSet<_> = agendas.iter().map(|(_, _, v)| v).flatten().collect();

    let agenda_ids = agenda_ids
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join("-");
    let practice_ids = practice_ids
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join("-");

    let visit_motive_ids = visit_motive_ids
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join("-");

    let center = Center::new(agenda_ids, practice_ids, visit_motive_ids);

    info!("{:?}", center);

    // Today's date "2021-05-21"
    let date = Utc::now()
        .checked_add_signed(chrono::Duration::days(days as i64))
        .unwrap();

    let test_date = date.format("%Y-%m-%d").to_string();

    info!("Test date {}", test_date);

    let available = center.check_availablity(&test_date).await?;

    let two_days_slots = available
        .pointer("/availabilities")
        .and_then(|x| x.as_array())
        .map(|x| {
            x.iter()
                .take(2) // next two days
                .filter_map(|y| y.pointer("/slots"))
                .filter_map(|x| x.as_array())
                .flatten()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // if !two_days_slots.is_empty() {
    //     info!("{:?}", center_url);
    //     info!("{:?}", center_url);
    //     info!("{:?}", two_days_slots);
    // }

    info!("slots available on {}: {}", test_date, two_days_slots.len());

    let a: Vec<Result<Option<String>>> = join_all(
        two_days_slots
            .into_iter()
            // .take(1)
            .filter_map(|x| {
                x.pointer("/start_date")
                    .and_then(|x| x.as_str())
                    .or_else(|| x.as_str())
            })
            .filter_map(|x| {
                DateTime::parse_from_str(x, "%Y-%m-%dT%H:%M:%S%.f%:z")
                    .or_else(|_| DateTime::parse_from_str(x, "%Y-%m-%dT%H:%M:%S%.f%z"))
                    .or_else(|_| {
                        NaiveDateTime::parse_from_str(x, "%Y-%m-%dT%H:%M:%S%.f")
                            .map(|ndt| DateTime::from_utc(ndt, FixedOffset::east(2 * 3600)))
                    })
                    .ok()
                    .map(|x| x.signed_duration_since(Utc::now()).num_minutes() <= 24 * 60) // chronodose (check appointments only in 24 hours)
                    .map(|_| x)
            })
            .map(|s| (center.clone(), s))
            .map(|(center, first_slot)| async move {
                let aps = center.check_appointment(&[first_slot]).await?;
                Ok(if aps[0].pointer("/error").is_some() {
                    // info!("response {}", serde_json::to_string_pretty(&aps[0]).unwrap());
                    info!("unavailable {}", first_slot);
                    None
                } else {
                    // info!(
                    //     "response {}",
                    //     serde_json::to_string_pretty(&aps[0]).unwrap()
                    // );
                    info!("available {}", first_slot);
                    Some(first_slot.to_string())
                })
            })
            .collect::<Vec<_>>(),
    )
    .await;

    let count = a.into_iter().flatten().flatten().count();

    info!(
        "{} has {} slots on {}",
        center_data
            .pointer("/data/profile/name_with_title")
            .and_then(|x| serde_json::to_string_pretty(x).ok())
            .unwrap(),
        count,
        test_date
    );

    Ok(count)
}
