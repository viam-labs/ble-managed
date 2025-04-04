//! Defines logic to grab values from environment.

use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

use anyhow::{anyhow, Result};
use log::warn;
use serde::Deserialize;

/// Path to default Viam config.
const VIAM_CONFIG_FP: &'static str = "/etc/viam.json";

/// Path to advertised BLE name file.
const ADVERTISED_BLE_NAME_FP: &'static str = "/etc/advertised_ble_name.txt";

/// Default advertised BLE name if none is specified at `ADVERSTISED_BLE_NAME_FILE`.
const DEFAULT_ADVERTISED_BLE_NAME: &'static str = "Viam SOCKS forwarder";

#[derive(Deserialize)]
struct ViamCloudConfig {
    cloud: Cloud,
}

#[derive(Deserialize)]
struct Cloud {
    // Other fields will exist in a Viam cloud config, but we only care about `id`.
    id: String,
}

/// Finds machine part ID from `VIAM_CONFIG_FILE`'s `id` field.
pub async fn get_machine_part_id() -> Result<String> {
    let viam_config_file = match File::open(Path::new(VIAM_CONFIG_FP)) {
        Ok(file) => file,
        _ => {
            return Err(anyhow!(
                "could not open file from file path \"{VIAM_CONFIG_FP}\"; ensure Viam cloud config is available at that location"
            ));
        }
    };
    let viam_cloud_config: ViamCloudConfig = match serde_json::from_reader(&viam_config_file) {
        Ok(vcc) => vcc,
        _ => {
            return Err(anyhow!(
                "contents of \"{VIAM_CONFIG_FP}\" did not contain a Viam cloud config with an `id` field; ensure cloud config is well formed"
            ));
        }
    };
    Ok(viam_cloud_config.cloud.id)
}

/// Finds name to advertise over BLE from `ADVERSTISED_BLE_NAME_FILE` or default value.
pub async fn get_advertised_ble_name() -> Result<String> {
    // Assume that advertised BLE name is present in first line of `ADVERTISED_BLE_NAME_FP`.
    let advertised_ble_name_file = match File::open(Path::new(ADVERTISED_BLE_NAME_FP)) {
        Ok(file) => file,
        _ => {
            warn!(
                "Could not open file from file path {ADVERTISED_BLE_NAME_FP:#?}; \
                    defaulting to \"{DEFAULT_ADVERTISED_BLE_NAME}\" as advertised name"
            );
            return Ok(DEFAULT_ADVERTISED_BLE_NAME.to_string());
        }
    };
    let mut lines = io::BufReader::new(advertised_ble_name_file).lines();

    match lines.next() {
        Some(Ok(line)) => Ok(line),
        _ => {
            warn!(
                "Could not read name to advertise from file path {ADVERTISED_BLE_NAME_FP:#?}); \
                    defaulting to \"{DEFAULT_ADVERTISED_BLE_NAME}\" as advertised name"
            );
            Ok(DEFAULT_ADVERTISED_BLE_NAME.to_string())
        }
    }
}
