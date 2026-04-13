use anyhow::Result;
use jsonc_parser::parse_to_serde_value;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct Config {
    pub trae_executable_path: String,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let raw_json_data = fs::read_to_string("config.jsonc")
            .expect("Cannot read configuration file `config.jsonc` in current working directory.");

        let config: Config = parse_to_serde_value(&raw_json_data, &Default::default())?;
        Ok(config)
    }
}
