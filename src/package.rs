use anyhow::{Context, Result, anyhow};
use std::io::prelude::*;

pub fn extract_application(path: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    extract(path, "application")
}

pub fn extract_softdevice_bootloader(path: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    extract(path, "softdevice_bootloader")
}

fn extract(path: &str, component: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let reader = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(reader)?;

    let manifest_raw = zip
        .by_name("manifest.json")
        .context("DFU package: missing manifest.json")?;
    let manifest: serde_json::Value = serde_json::from_reader(manifest_raw)?;

    let dat = extract_part(&mut zip, &manifest, component, "dat_file")?;
    let bin = extract_part(&mut zip, &manifest, component, "bin_file")?;

    Ok((dat, bin))
}

fn extract_part(
    zip: &mut zip::ZipArchive<std::fs::File>,
    manifest: &serde_json::Value,
    component: &str,
    part: &str,
) -> Result<Vec<u8>> {
    let comp = &manifest["manifest"][component];
    anyhow::ensure!(comp.is_object(), "DFU package: missing component `{}`", component);
    let part_name = comp[part].as_str().ok_or(anyhow!("DFU package: invalid manifest"))?;

    let mut reader = zip.by_name(part_name).context("invalid DFU package")?;
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;

    Ok(data)
}
