use std::fs;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all="lowercase")]
pub enum MqttType {
    Source,
    Sink
}
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub log_level: Option<String>,
    pub servers: Vec<MqttServer>
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttServer {
    pub name: String,
    pub mqtt_type: MqttType,
    pub address: String,
    pub port: u16,
    pub client_name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub topic: String,
    pub suffix_name: Option<bool>
}

impl AppConfig {
    pub fn from_file(cfg_file: String) -> Self {
        let yaml = fs::read_to_string(cfg_file).expect("Could not read config file");
        let cfg: AppConfig = serde_yaml::from_str(&yaml).expect("Config parse error");
        cfg
    }
}