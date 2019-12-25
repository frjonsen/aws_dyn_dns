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

struct AwsDynDns {
    current_ip: String,
    config: Config,
    client: Route53Client
}

impl AwsDynDns {

    fn new(config: Config, client: Route53Client) -> Self {
        let current_ip = AwsDynDns::get_current_ip().expect("Failed to get current IP");

        AwsDynDns {
            current_ip,
            config,
            client
        }
    }
    fn get_current_ip() -> Result<String, Box<dyn std::error::Error>> {
        let ip_result: HashMap<String, String> = reqwest::get("https://api.ipify.org?format=json")?.json()?;
        Ok(ip_result.get("ip").expect("Failed to retrieve current IP").clone())
    }

    fn create_a_record(&self, domain: &str) -> ResourceRecordSet {
        info!("Creating record for host {}", &domain);
        let mut resource_record = ResourceRecordSet::default();
        resource_record.name = domain.to_owned();
        resource_record.type_ = "A".to_owned();
        resource_record.ttl = Some(600);
        resource_record.resource_records = Some(vec![
            ResourceRecord {
                value: self.current_ip.clone()
            }
        ]);

        resource_record
    }

    fn domains_to_change(&self) -> ChangeBatch {
        info!("Updating records to point to {}", &self.current_ip);

        let changes = self.config.records.iter().map(|d| Change {
            action: "UPSERT".to_owned(),
            resource_record_set: self.create_a_record(d)
        }).collect::<Vec<_>>();

        ChangeBatch {
            changes,
            comment: None
        }
    }

    fn do_update(&self) {
        let changes = self.domains_to_change();
        let request = ChangeResourceRecordSetsRequest {
            hosted_zone_id: self.config.hosted_zone_id.clone(),
            change_batch: changes
        };
        self.client.change_resource_record_sets(request).sync()
        .expect("API call failed");
    }

    pub fn update_records(&self) {
        info!("Updating records {}", &self.config.records.join(", "));
        self.do_update();
    }
}

fn get_config_path() -> PathBuf {
    let xdg_config = env::var("XDG_CONFIG_HOME");
    let config_dir = xdg_config
    .or_else(|_| env::var("HOME").map(|h| format!("{}/.config", h)))
    .expect("Unable to find config directory");
    let path = Path::new(&config_dir);
    path.join(Path::new("awsdyndns/config.json"))
}

fn read_config() -> Config  {
    let path = get_config_path();
    if !path.exists() {
        error!("No config file at {}", &path.to_string_lossy());
        panic!("Found no config file");
    }
    let config_file = File::open(path).expect("Failed to open config file");
    let reader = BufReader::new(config_file);
    serde_json::from_reader(reader).expect("Failed to parse config file")
}

fn main() {
    env_logger::init();
    let client = Route53Client::new(Region::UsEast1);
    AwsDynDns::new(read_config(), client).update_records();
}
