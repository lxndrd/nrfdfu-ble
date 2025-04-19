use std::io::prelude::*;

pub fn extract_application(path: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    extract(path, "application")
}

pub fn extract_softdevice_bootloader(path: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    extract(path, "softdevice_bootloader")
}

fn extract(path: &str, component: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    let reader = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(reader)?;

    let manifest_raw = zip.by_name("manifest.json")?;
    let manifest: serde_json::Value = serde_json::from_reader(manifest_raw)?;

    let part = &manifest["manifest"][component];
    let dat_name = part["dat_file"].as_str().unwrap();
    let bin_name = part["bin_file"].as_str().unwrap();

    let mut dat = Vec::new();
    zip.by_name(dat_name)?.read_to_end(&mut dat)?;

    let mut bin = Vec::new();
    zip.by_name(bin_name)?.read_to_end(&mut bin)?;

    Ok((dat, bin))
}
