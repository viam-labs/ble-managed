//! Defines logic to grab values from environment.

use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde::Deserialize;

/// Path to default Viam config.
const VIAM_CONFIG_FP: &'static str = "/etc/viam.json";

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
