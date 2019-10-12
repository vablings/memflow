use dirs;
use log::{info, trace, warn};
use std::fs;
use std::io::{Cursor, Error, ErrorKind, Result};
use std::path::PathBuf;

use byteorder::ReadBytesExt;
use byteorder::{ByteOrder, LittleEndian};
use clap::ArgMatches;
use duma;
use goblin::pe::{debug::CodeviewPDB70DebugInfo, PE};
use uuid::{BytesError, Uuid};

fn cache_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| Error::new(ErrorKind::Other, "unable to get home directory"))?;
    let cache = home.join(".memflow").join("cache");
    Ok(cache)
}

fn download_pdb(pdbname: &str, guid: &String) -> Result<()> {
    info!("downloading pdb for {} with guid/age {}", pdbname, guid);

    let url = match duma::utils::parse_url(&format!(
        "https://msdl.microsoft.com/download/symbols/{}/{}/{}",
        pdbname, guid, pdbname
    )) {
        Ok(u) => u,
        Err(e) => {
            return Err(Error::new(
                ErrorKind::Other,
                format!("unable to format download url: {}", e),
            ))
        }
    };

    match duma::download::http_download(url, &ArgMatches::default(), "0.1") {
        Ok(_) => Ok(()),
        Err(e) => Err(Error::new(
            ErrorKind::Other,
            format!("unable to download pdb file: {}", e),
        )),
    }
}

fn download_pdb_cache(pdbname: &str, guid: &String) -> Result<PathBuf> {
    info!("fetching pdb for {} with guid/age {}", pdbname, guid);

    let cache_dir = cache_dir()?.join(pdbname);
    let cache_file = cache_dir.join(guid);
    if !cache_file.exists() {
        info!(
            "{} does not exist in cache, downloading from microsoft servers",
            pdbname
        );
        download_pdb(pdbname, guid)?;

        // create cache dir if necessary and move the downloaded file
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        // TODO: this is super dirty as we cannot decide where to put the resulting file
        // a fork of duma would be necessary to add decent library functionality!
        info!(
            "moving {} to cache {}",
            pdbname,
            cache_file.to_str().unwrap_or_default()
        );
        fs::rename(pdbname, &cache_file)?;
    }

    Ok(cache_file)
}

// TODO: this function might be omitted in the future if this is merged to goblin internally
fn generate_guid(codeview: &CodeviewPDB70DebugInfo) -> std::result::Result<String, BytesError> {
    let mut rdr = Cursor::new(codeview.signature);
    let uuid = Uuid::from_fields(
        rdr.read_u32::<LittleEndian>().unwrap_or_default(), // TODO: fix error handling
        rdr.read_u16::<LittleEndian>().unwrap_or_default(),
        rdr.read_u16::<LittleEndian>().unwrap_or_default(),
        &codeview.signature[8..],
    )?;

    Ok(format!(
        "{}{:X}",
        uuid.to_simple().to_string().to_uppercase(),
        codeview.age
    ))
}

pub fn fetch_pdb(pe: &PE) -> Result<PathBuf> {
    let debug = match pe.debug_data {
        Some(d) => d,
        None => {
            return Err(Error::new(
                ErrorKind::Other,
                "unable to read debug_data in pe header",
            ))
        }
    };

    let codeview = match debug.codeview_pdb70_debug_info {
        Some(c) => c,
        None => {
            return Err(Error::new(
                ErrorKind::Other,
                "unable to read codeview in debug header",
            ))
        }
    };

    // TODO: map error of generate_guid properly
    download_pdb_cache(
        &String::from_utf8(codeview.filename.to_vec())
            .unwrap_or_default()
            .trim_matches(char::from(0)),
        &generate_guid(&codeview).unwrap_or_default(),
    )
}
