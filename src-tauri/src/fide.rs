use std::{
    fs::{remove_file, File},
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use bincode::{config, Decode, Encode};
use quick_xml::de::from_reader;
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use strsim::{jaro_winkler, sorensen_dice};
use tauri::{path::BaseDirectory, Manager};
use tauri_specta::Event;

use crate::{error::Error, fs::DownloadProgress};
use crate::{fs::download_file, AppState};

#[derive(Debug, Deserialize, Serialize, Type, Clone, Decode, Encode)]
pub struct FidePlayer {
    pub fideid: u32,
    pub name: String,
    pub country: String,
    pub sex: String,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub title: Option<String>,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub w_title: Option<String>,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub o_title: Option<String>,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub foa_title: Option<String>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub rating: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub games: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub k: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub rapid_rating: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub rapid_games: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub rapid_k: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub blitz_rating: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub blitz_games: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub blitz_k: Option<u16>,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub birthday: Option<u16>,
    #[serde(deserialize_with = "empty_string_is_none")]
    pub flag: Option<String>,
}

fn empty_string_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(s))
    }
}

fn deserialize_option_u16<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::deserialize(deserializer).unwrap_or(None))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayersList {
    #[serde(rename = "player")]
    pub players: Vec<FidePlayer>,
}

const FIDE_XML: &str = "players_list_xml_foa.xml";

/// Converts the unzipped FIDE xml in `dir` to `fide.bin` and removes the xml.
fn convert_fide_xml(dir: &Path) -> Result<Vec<FidePlayer>, Error> {
    let xml_path = dir.join(FIDE_XML);
    let players_list: PlayersList = from_reader(BufReader::new(File::open(&xml_path)?))?;

    let out_path = dir.join("fide.bin");
    let part_path = dir.join("fide.bin.part");
    let written = (|| -> Result<(), Error> {
        let mut out_file = BufWriter::new(File::create(&part_path)?);
        bincode::encode_into_std_write(&players_list.players, &mut out_file, config::standard())?;
        out_file.flush()?;
        out_file.get_ref().sync_all()?;
        std::fs::rename(&part_path, &out_path)?;
        Ok(())
    })();
    if let Err(e) = written {
        let _ = remove_file(&part_path);
        return Err(e);
    }

    remove_file(&xml_path)?;
    Ok(players_list.players)
}

#[tauri::command]
#[specta::specta]
pub async fn download_fide_db(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), Error> {
    let data_dir = app.path().app_data_dir()?;

    download_file(
        "fide_db".to_string(),
        "https://ratings.fide.com/download/players_list_xml.zip".to_string(),
        data_dir.clone(),
        app.clone(),
        None,
        Some(false),
        None,
    )
    .await?;

    let players = convert_fide_xml(&data_dir)?;
    *state.fide_players.write().await = players;

    DownloadProgress {
        progress: 100.0,
        id: "fide_db".to_string(),
        finished: true,
    }
    .emit(&app)?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn find_fide_player(
    player: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Option<FidePlayer>, Error> {
    let fide_players = state.fide_players.read().await;

    if fide_players.is_empty() {
        drop(fide_players);
        let config = config::standard();
        let fide_path = app.path().resolve("fide.bin", BaseDirectory::AppData)?;

        if let Ok(f) = File::open(&fide_path) {
            let mut fide_players = state.fide_players.write().await;
            *fide_players = bincode::decode_from_reader(BufReader::new(f), config)?;
        }
    }

    let fide_players = state.fide_players.read().await;
    let mut best_match = None;
    let mut best_match_score = 0.0;

    for fide_player in (*fide_players).iter() {
        let sorenson_score = sorensen_dice(&player, &fide_player.name);
        let jaro_score = jaro_winkler(&player, &fide_player.name);
        let score = sorenson_score.max(jaro_score);
        if score > best_match_score {
            best_match = Some(fide_player);
            best_match_score = score;
        }
    }

    if best_match_score > 0.8 {
        Ok(best_match.cloned())
    } else {
        Err(Error::NoMatchFound)
    }
}

#[cfg(test)]
mod tests {
    use super::{convert_fide_xml, FidePlayer, FIDE_XML};
    use crate::fs::{extract_atomic, ArchiveKind};
    use bincode::config;
    use std::io::{Cursor, Write};
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    const XML: &str = "<playerslist><player><fideid>1503014</fideid><name>Carlsen, Magnus</name>\
<country>NOR</country><sex>M</sex><title>GM</title><w_title></w_title><o_title></o_title>\
<foa_title>IA</foa_title><rating>2830</rating><games>0</games><k>10</k>\
<rapid_rating>2800</rapid_rating><rapid_games>0</rapid_games><rapid_k>10</rapid_k>\
<blitz_rating></blitz_rating><blitz_games></blitz_games><blitz_k></blitz_k>\
<birthday>1990</birthday><flag>wi</flag></player></playerslist>";

    #[test]
    fn unzipped_fide_list_becomes_fide_bin() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(FIDE_XML, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(XML.as_bytes()).unwrap();
        let zip = writer.finish().unwrap().into_inner();
        let dir = tempdir().unwrap();

        extract_atomic(dir.path(), ArchiveKind::Zip, zip).unwrap();
        let players = convert_fide_xml(dir.path()).unwrap();

        let bytes = std::fs::read(dir.path().join("fide.bin")).unwrap();
        let (decoded, _): (Vec<FidePlayer>, _) =
            bincode::decode_from_slice(&bytes, config::standard()).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].fideid, 1503014);
        assert_eq!(decoded[0].rating, Some(2830));
        assert_eq!(decoded[0].blitz_rating, None);
        assert_eq!(players[0].name, "Carlsen, Magnus");
        assert!(!dir.path().join(FIDE_XML).exists());
    }

    #[test]
    fn failed_conversion_keeps_existing_fide_bin() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("fide.bin"), b"good").unwrap();
        std::fs::write(dir.path().join(FIDE_XML), b"<playerslist><player>").unwrap();

        assert!(convert_fide_xml(dir.path()).is_err());

        assert_eq!(std::fs::read(dir.path().join("fide.bin")).unwrap(), b"good");
        assert!(!dir.path().join("fide.bin.part").exists());
    }
}
