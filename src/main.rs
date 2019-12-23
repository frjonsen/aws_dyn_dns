#[macro_use]
extern crate log;
use rusoto_core::Region;
use rusoto_route53::{
    Change,
    Route53Client,
    ChangeResourceRecordSetsRequest,
    ResourceRecordSet,
    ResourceRecord,
    ChangeBatch
};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::BufReader;
use std::env;
use serde::Deserialize;
use rusoto_route53::Route53;
use std::vec::Vec;
use reqwest;
use std::collections::HashMap;
use env_logger;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Config {
    hosted_zone_id: String,
    records: Vec<String>
}

fn get_config_path() -> PathBuf {
    let config_dir = match env::var("XDG_CONFIG_HOME") {
        Ok(s) => s,
        _ => format!("{}/.config", env::var("HOME").unwrap())
    };
    let path = Path::new(&config_dir);
    path.join(Path::new("awsdyndns/config.json"))
}

fn get_current_ip() -> Result<String, Box<dyn std::error::Error>> {
    let ip_result: HashMap<String, String> = reqwest::get("https://api.ipify.org?format=json")?.json()?;
    Ok(ip_result.get("ip").unwrap().clone())
}

fn domains_to_change(domains: &Vec<String>) -> ChangeBatch {
    let current_ip = get_current_ip().expect("Unable to retrieve current IP");
    let mut changes: Vec<Change> = Vec::new();

    for b in domains {
        info!("Updating domain {} to resource {}", &b, &current_ip);
        let mut resource_record = ResourceRecordSet::default();
        resource_record.name = b.clone(); 
        resource_record.type_ = "A".to_owned();
        resource_record.ttl = Some(600);
        resource_record.resource_records = Some(vec![
            ResourceRecord {
                value: current_ip.clone()
            }
        ]);
        changes.push(Change {
            action: "UPSERT".to_owned(),
            resource_record_set: resource_record
        });
    }

    ChangeBatch {
        changes,
        comment: None
    }
}

fn read_config() -> Config  {
    let path = get_config_path();
    let config_file = File::open(path).unwrap();
    let reader = BufReader::new(config_file);
    serde_json::from_reader(reader).unwrap()
}

fn main() {
    env_logger::init();
    let config = read_config();
    info!("Updating records {}", &config.records.join(", "));
    info!("Updating to ip {}", get_current_ip().unwrap());
    let changes = domains_to_change(&config.records);
    let client = Route53Client::new(Region::UsEast1);
    let request = ChangeResourceRecordSetsRequest {
        hosted_zone_id: config.hosted_zone_id,
        change_batch: changes
    };
    client.change_resource_record_sets(request).sync().unwrap();
}
