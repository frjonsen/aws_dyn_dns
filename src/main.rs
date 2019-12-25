#[macro_use]
extern crate log;
use rusoto_core::Region;
use rusoto_route53::{
    Change,
    Route53Client,
    ChangeResourceRecordSetsRequest,
    ListResourceRecordSetsRequest,
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

#[derive(Debug)]
struct Record {
    domain: String,
    resource: String
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

    fn record_set_to_internal_record(record: &ResourceRecordSet) -> Result<Record, &str> {
        Ok(Record {
            domain: record.name.replace("\\052", "*"),
            resource: AwsDynDns::get_first_resource_from_record(&record)?
        })
    }

    fn get_first_resource_from_record(record: &ResourceRecordSet) -> Result<String, &str> {
        Ok(record.resource_records.as_ref().and_then(|r| r.get(0)).map(|r| r.value.clone()).ok_or("Record has no resource")?)
    }

    fn get_current_records(&self) -> Result<Vec<Record>, Box<dyn std::error::Error>> {
        let mut request = ListResourceRecordSetsRequest::default();
        request.hosted_zone_id = self.config.hosted_zone_id.clone();

        let records = self.client.list_resource_record_sets(request).sync()?;

        Ok(records.resource_record_sets.iter().filter(|h| h.type_ == "A").map(AwsDynDns::record_set_to_internal_record).filter_map(Result::ok).collect::<Vec<_>>())

    }

    fn filter_up_to_date_records(&self) -> Vec<&String> {
        if let Ok(a_records) = self.get_current_records() {
            self.config.records.iter().filter(|r| a_records.iter().find(|l| l.domain.starts_with(*r) && l.resource == self.current_ip).is_none()).collect::<Vec<_>>()
        } else {
            warn!("Failed to get existing records. Updating all specified host names");
            self.config.records.iter().collect::<Vec<_>>()
        }
    }

    fn domains_to_change(&self) -> ChangeBatch {
        info!("Updating records to point to {}", &self.current_ip);
        let stale_records = self.filter_up_to_date_records();

        let changes = stale_records.iter().map(|d| Change {
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
        if changes.changes.len() == 0 {
            info!("No stale records. Exiting without making any changes");
            return;
        }
        let request = ChangeResourceRecordSetsRequest {
            hosted_zone_id: self.config.hosted_zone_id.clone(),
            change_batch: changes
        };
        info!("Sending API update call");
        self.client.change_resource_record_sets(request).sync()
        .expect("API call failed");
    }

    pub fn update_records(&self) {
        info!("Updating records {}", &self.config.records.join(", "));
        self.do_update();
    }
}

fn get_config_path() -> PathBuf {
    env::var("XDG_CONFIG_HOME")
    .or_else(|_| env::var("HOME").map(|h| format!("{}/.config", h)))
    .map(|c| Path::new(&c).join(Path::new("awsdyndns/config.json")))
    .expect("Unable to find config directory")
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
