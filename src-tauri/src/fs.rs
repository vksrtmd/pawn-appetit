use std::{
    fs::create_dir_all,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use flate2::read::GzDecoder;
use log::info;
use reqwest::{Client, Url};
use specta::Type;
use tar::EntryType;
use tauri_specta::Event;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use futures_util::{Stream, StreamExt};
use tauri::Manager;

use crate::error::Error;

const MAX_DOWNLOAD_SIZE: u64 = 10 * 1024 * 1024 * 1024;

#[derive(Clone, Type, serde::Serialize, Event)]
pub struct DownloadProgress {
    pub progress: f32,
    pub id: String,
    pub finished: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn download_file(
    id: String,
    url: String,
    path: PathBuf,
    app: tauri::AppHandle,
    token: Option<String>,
    finalize: Option<bool>,
    total_size: Option<f64>,
) -> Result<(), Error> {
    let finalize = finalize.unwrap_or(true);

    // Convert f64 to u64 if total_size is provided
    let total_size_u64 = total_size.and_then(|size| {
        if size >= 0.0 && size <= u64::MAX as f64 {
            Some(size as u64)
        } else {
            None
        }
    });

    let parsed_url =
        Url::parse(&url).map_err(|e| Error::PackageManager(format!("Invalid URL: {}", e)))?;

    if parsed_url.scheme() != "https" && parsed_url.scheme() != "http" {
        return Err(Error::PackageManager(format!(
            "Only HTTP/HTTPS allowed, got: {}",
            parsed_url.scheme()
        )));
    }

    let path = ensure_inside(&app.path().app_data_dir()?, &path)?;

    info!("Downloading file from {} to {}", url, path.display());

    // No whole-request timeout: large engine downloads legitimately take longer than any fixed cap.
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .build()?;

    let mut req = client.get(&url);

    if let Some(token) = token {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let res = req.send().await?;

    if !res.status().is_success() {
        return Err(Error::PackageManager(format!(
            "Download failed: {}",
            res.status()
        )));
    }

    let content_length = total_size_u64.or_else(|| res.content_length());

    if let Some(size) = content_length {
        if size > MAX_DOWNLOAD_SIZE {
            return Err(Error::PackageManager(format!(
                "File too large: {} bytes (max {})",
                size, MAX_DOWNLOAD_SIZE
            )));
        }
    }

    let emit = |progress: f32, finished: bool| {
        DownloadProgress {
            progress,
            id: id.clone(),
            finished,
        }
        .emit(&app)
        .map_err(Error::from)
    };

    if let Some(kind) = ArchiveKind::from_url(&url) {
        // The download is the first half of the progress bar, extraction the second.
        let data = collect_stream(res.bytes_stream(), content_length, |p| {
            emit(if p < 0.0 { p } else { p / 2.0 }, false)
        })
        .await?;
        emit(50.0, false)?;
        extract_atomic(&path, kind, data)?;
        info!("Extraction complete");
    } else {
        download_atomic(res.bytes_stream(), &path, content_length, |p| {
            emit(p, false)
        })
        .await?;
        info!("Downloaded file to {}", path.display());
    }

    if finalize {
        emit(100.0, true)?;
    }

    Ok(())
}

/// Returns `target` with symlinks in its existing part resolved, if it lies inside `base`.
pub fn ensure_inside(base: &Path, target: &Path) -> Result<PathBuf, Error> {
    let outside = || {
        Error::PackageManager(format!(
            "Path is outside the app data folder: {}",
            target.display()
        ))
    };

    if !target.is_absolute() || target.components().any(|c| c == Component::ParentDir) {
        return Err(outside());
    }

    let base = base.canonicalize()?;

    // Walk up to the nearest ancestor that exists, so a target that is not created yet can be checked.
    let mut existing = target;
    let mut missing = Vec::new();
    let mut resolved = loop {
        // A dangling symlink fails canonicalize but would still be followed on write.
        if existing.symlink_metadata().is_ok() {
            break existing.canonicalize()?;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                existing = parent;
            }
            _ => return Err(outside()),
        }
    };
    resolved.extend(missing.iter().rev());

    if resolved.starts_with(&base) {
        Ok(resolved)
    } else {
        Err(outside())
    }
}

/// Feeds every chunk to `sink`, reporting percent complete (-1 when the total is unknown).
async fn drain_stream<S, C, E>(
    stream: S,
    total: Option<u64>,
    mut sink: impl FnMut(&[u8]) -> Result<(), Error>,
    mut on_progress: impl FnMut(f32) -> Result<(), Error>,
) -> Result<(), Error>
where
    S: Stream<Item = Result<C, E>>,
    C: AsRef<[u8]>,
    E: Into<Error>,
{
    futures_util::pin_mut!(stream);
    let mut downloaded: u64 = 0;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(Into::into)?;
        let chunk = chunk.as_ref();

        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_DOWNLOAD_SIZE {
            return Err(Error::PackageManager(
                "Download size limit exceeded".to_string(),
            ));
        }

        sink(chunk)?;

        on_progress(
            total
                .map(|total| ((downloaded as f64 / total as f64) * 100.0).min(100.0) as f32)
                .unwrap_or(-1.0),
        )?;
    }

    Ok(())
}

async fn collect_stream<S, C, E>(
    stream: S,
    total: Option<u64>,
    on_progress: impl FnMut(f32) -> Result<(), Error>,
) -> Result<Vec<u8>, Error>
where
    S: Stream<Item = Result<C, E>>,
    C: AsRef<[u8]>,
    E: Into<Error>,
{
    let mut data = Vec::with_capacity(total.unwrap_or(0).min(MAX_DOWNLOAD_SIZE) as usize);
    drain_stream(
        stream,
        total,
        |chunk| {
            data.extend_from_slice(chunk);
            Ok(())
        },
        on_progress,
    )
    .await?;
    Ok(data)
}

/// Streams into `<path>.part` and renames over `path` on success; on error `path` is untouched.
async fn download_atomic<S, C, E>(
    stream: S,
    path: &Path,
    total: Option<u64>,
    on_progress: impl FnMut(f32) -> Result<(), Error>,
) -> Result<(), Error>
where
    S: Stream<Item = Result<C, E>>,
    C: AsRef<[u8]>,
    E: Into<Error>,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);

    let result = async {
        let mut file = std::fs::File::create(&part)?;
        drain_stream(
            stream,
            total,
            |chunk| Ok(file.write_all(chunk)?),
            on_progress,
        )
        .await?;
        file.sync_all()?;
        std::fs::rename(&part, path)?;
        Ok(())
    }
    .await;

    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

#[derive(Clone, Copy)]
pub enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
}

impl ArchiveKind {
    fn from_url(url: &str) -> Option<Self> {
        if url.ends_with(".zip") {
            Some(Self::Zip)
        } else if url.ends_with(".tar") {
            Some(Self::Tar)
        } else if url.ends_with(".tar.gz") {
            Some(Self::TarGz)
        } else {
            None
        }
    }
}

/// Extracts into a scratch dir inside `target` (same filesystem, so the moves are renames), then
/// moves each top-level entry over `target`. On an extraction error `target` is untouched.
pub fn extract_atomic(target: &Path, kind: ArchiveKind, data: Vec<u8>) -> Result<(), Error> {
    create_dir_all(target)?;
    let stage = tempfile::Builder::new()
        .prefix(".extract-")
        .tempdir_in(target)?;

    match kind {
        ArchiveKind::Zip => unzip_file(stage.path(), data)?,
        ArchiveKind::Tar => extract_tar_file(stage.path(), data, false)?,
        ArchiveKind::TarGz => extract_tar_file(stage.path(), data, true)?,
    }

    for entry in std::fs::read_dir(stage.path())? {
        let entry = entry?;
        let dest = target.join(entry.file_name());
        if let Ok(meta) = dest.symlink_metadata() {
            if meta.is_dir() {
                std::fs::remove_dir_all(&dest)?;
            } else {
                std::fs::remove_file(&dest)?;
            }
        }
        std::fs::rename(entry.path(), &dest)?;
    }

    Ok(())
}

fn unsafe_archive_error(message: impl Into<String>) -> Error {
    Error::PackageManager(format!("Unsafe archive entry: {}", message.into()))
}

fn validate_archive_relative_path(path: &Path) -> Result<(), Error> {
    let mut has_normal_component = false;

    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(unsafe_archive_error(format!(
                    "parent-directory traversal in {}",
                    path.display()
                )));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(unsafe_archive_error(format!(
                    "absolute path in {}",
                    path.display()
                )));
            }
        }
    }

    if !has_normal_component {
        return Err(unsafe_archive_error("empty archive path"));
    }

    Ok(())
}

fn ensure_canonical_path_inside(base_path: &Path, path: &Path) -> Result<(), Error> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(base_path) {
        return Err(unsafe_archive_error(format!(
            "entry escapes destination: {}",
            path.display()
        )));
    }

    Ok(())
}

fn prepare_archive_output_file(base_path: &Path, outpath: &Path) -> Result<(), Error> {
    let parent = outpath.parent().ok_or_else(|| {
        unsafe_archive_error(format!("entry has no parent: {}", outpath.display()))
    })?;
    create_dir_all(parent)?;
    ensure_canonical_path_inside(base_path, parent)?;

    if let Ok(metadata) = std::fs::symlink_metadata(outpath) {
        if metadata.file_type().is_symlink() {
            return Err(unsafe_archive_error(format!(
                "refusing to overwrite symlink: {}",
                outpath.display()
            )));
        }
    }

    Ok(())
}

fn validate_zip_entry(file: &zip::read::ZipFile<'_>) -> Result<(), Error> {
    if let Some(mode) = file.unix_mode() {
        validate_archive_mode(mode, file.name())?;

        let file_type = mode & 0o170000;
        let is_regular = file_type == 0 || file_type == 0o100000;
        let is_directory = file_type == 0o040000;
        if !(is_regular || is_directory) {
            return Err(unsafe_archive_error(format!(
                "unsupported ZIP entry type for {}",
                file.name()
            )));
        }
    }

    Ok(())
}

fn validate_tar_entry<R: Read>(entry: &tar::Entry<'_, R>) -> Result<(), Error> {
    let entry_type = entry.header().entry_type();
    if !is_supported_tar_entry_type(entry_type) {
        return Err(unsafe_archive_error(format!(
            "unsupported TAR entry type {:?} for {}",
            entry_type,
            entry.path()?.display()
        )));
    }

    if let Ok(mode) = entry.header().mode() {
        validate_archive_mode(mode, &entry.path()?.display().to_string())?;
    }

    if entry_type.is_hard_link() || entry_type.is_symlink() {
        let link_name = entry.link_name()?;
        return Err(unsafe_archive_error(format!(
            "refusing archive link {} -> {}",
            entry.path()?.display(),
            link_name
                .as_deref()
                .map(Path::display)
                .map(|display| display.to_string())
                .unwrap_or_else(|| "<missing>".to_string())
        )));
    }

    Ok(())
}

fn is_supported_tar_entry_type(entry_type: EntryType) -> bool {
    entry_type.is_file()
        || entry_type.is_dir()
        || entry_type.is_pax_global_extensions()
        || entry_type.is_pax_local_extensions()
        || entry_type.is_gnu_longname()
        || entry_type.is_gnu_longlink()
}

fn validate_archive_mode(mode: u32, entry_name: &str) -> Result<(), Error> {
    if mode & 0o7000 != 0 {
        return Err(unsafe_archive_error(format!(
            "dangerous permission bits on {}",
            entry_name
        )));
    }

    Ok(())
}

fn set_safe_file_permissions(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        std::fs::set_permissions(path, Permissions::from_mode(0o644))?;
    }

    Ok(())
}

fn set_safe_directory_permissions(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        std::fs::set_permissions(path, Permissions::from_mode(0o755))?;
    }

    Ok(())
}

pub fn unzip_file(path: &Path, file: Vec<u8>) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file))?;

    create_dir_all(path)?;
    let base_path = path.canonicalize()?;
    let archive_len = archive.len();

    for i in 0..archive_len {
        let mut file = archive.by_index(i)?;
        validate_zip_entry(&file)?;

        let file_path = file.enclosed_name().ok_or_else(|| {
            Error::PackageManager(format!(
                "Invalid file path in archive at index {}: {:?}",
                i,
                file.name()
            ))
        })?;
        validate_archive_relative_path(&file_path)?;

        let outpath = base_path.join(file_path);

        if !outpath.starts_with(&base_path) {
            return Err(unsafe_archive_error(format!(
                "entry escapes destination: {}",
                file.name()
            )));
        }

        if file.is_dir() {
            info!("Creating directory from archive: \"{}\"", outpath.display());
            create_dir_all(&outpath)?;
            ensure_canonical_path_inside(&base_path, &outpath)?;
            set_safe_directory_permissions(&outpath)?;
        } else {
            let file_size = file.size();
            info!(
                "Extracting file {} of {}: \"{}\" ({} bytes)",
                i + 1,
                archive_len,
                outpath.display(),
                file_size
            );

            prepare_archive_output_file(&base_path, &outpath)?;

            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
            outfile.sync_all()?;
            set_safe_file_permissions(&outpath)?;
        }
    }

    Ok(())
}

fn extract_tar_file(path: &Path, file: Vec<u8>, gzipped: bool) -> Result<(), Error> {
    if gzipped {
        extract_tar_reader(path, GzDecoder::new(Cursor::new(file)))
    } else {
        extract_tar_reader(path, Cursor::new(file))
    }
}

fn extract_tar_reader<R: Read>(path: &Path, reader: R) -> Result<(), Error> {
    let mut archive = tar::Archive::new(reader);

    create_dir_all(path)?;
    let base_path = path.canonicalize()?;

    archive.set_overwrite(true);
    archive.set_preserve_mtime(false);
    archive.set_preserve_ownerships(false);
    archive.set_preserve_permissions(false);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        validate_archive_relative_path(&entry_path)?;
        validate_tar_entry(&entry)?;

        entry.set_preserve_mtime(false);
        entry.set_preserve_permissions(false);
        entry.set_mask(0o022);

        info!(
            "Extracting from tar: \"{}\" ({} bytes)",
            base_path.join(&entry_path).display(),
            entry.size()
        );

        if !entry.unpack_in(&base_path)? {
            return Err(unsafe_archive_error(format!(
                "entry escapes destination: {}",
                entry_path.display()
            )));
        }
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_file_as_executable(path: String, app: tauri::AppHandle) -> Result<(), Error> {
    let path = ensure_inside(&app.path().app_data_dir()?, Path::new(&path))?;
    let path = path.as_path();

    if !path.exists() {
        return Err(Error::PackageManager(format!(
            "File does not exist: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(Error::PackageManager(format!(
            "Not a file: {}",
            path.display()
        )));
    }

    #[cfg(unix)]
    {
        let metadata = std::fs::metadata(path)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions)?;
        info!("Set file as executable: {}", path.display());
    }

    #[cfg(not(unix))]
    {
        log::warn!(
            "set_file_as_executable called on Windows for: {}",
            path.display()
        );
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn file_exists(path: String) -> Result<bool, Error> {
    Ok(Path::new(&path).exists())
}

#[derive(Debug, Type, serde::Serialize)]
pub struct FileMetadata {
    pub last_modified: u64,
    pub size: u64,
    pub is_dir: bool,
    pub is_readonly: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_metadata(path: String) -> Result<FileMetadata, Error> {
    let path = Path::new(&path);

    if !path.exists() {
        return Err(Error::PackageManager(format!(
            "File does not exist: {}",
            path.display()
        )));
    }

    let metadata = std::fs::metadata(path)?;
    let last_modified = metadata
        .modified()?
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?;

    Ok(FileMetadata {
        last_modified: last_modified.as_secs(),
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        is_readonly: metadata.permissions().readonly(),
    })
}

#[cfg(test)]
mod fs_tests {
    use super::{
        download_atomic, ensure_inside, extract_atomic, extract_tar_file, unzip_file, ArchiveKind,
    };
    use crate::error::Error;
    use flate2::{write::GzEncoder, Compression};
    use futures_util::stream;
    use std::{
        io::{Cursor, Write},
        path::Path,
    };
    use tar::{Builder, EntryType, Header};
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn make_zip(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);

        for (name, content, mode) in entries {
            let mut options = SimpleFileOptions::default();
            if let Some(mode) = mode {
                options = options.unix_permissions(*mode);
            }
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }

        writer.finish().unwrap().into_inner()
    }

    fn set_header_path_literal(header: &mut Header, path: &str) {
        let path_bytes = path.as_bytes();
        assert!(path_bytes.len() < header.as_old().name.len());
        header.as_old_mut().name = [0; 100];
        header.as_old_mut().name[..path_bytes.len()].copy_from_slice(path_bytes);
    }

    fn make_tar_entry(path: &str, entry_type: EntryType, content: &[u8]) -> Vec<u8> {
        let mut archive = Vec::new();
        {
            let mut builder = Builder::new(&mut archive);
            let mut header = Header::new_old();
            header.set_entry_type(entry_type);
            header.set_mode(0o644);
            header.set_size(content.len() as u64);
            set_header_path_literal(&mut header, path);
            header.set_cksum();
            builder.append(&header, Cursor::new(content)).unwrap();
            builder.finish().unwrap();
        }
        archive
    }

    fn make_tar_link(path: &str, entry_type: EntryType, link_name: &str) -> Vec<u8> {
        let mut archive = Vec::new();
        {
            let mut builder = Builder::new(&mut archive);
            let mut header = Header::new_old();
            header.set_entry_type(entry_type);
            header.set_mode(0o644);
            header.set_size(0);
            header.set_link_name(Path::new(link_name)).unwrap();
            set_header_path_literal(&mut header, path);
            header.set_cksum();
            builder
                .append(&header, Cursor::new(Vec::<u8>::new()))
                .unwrap();
            builder.finish().unwrap();
        }
        archive
    }

    fn gzip(data: Vec<u8>) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn extracts_normal_zip_archive() {
        let dir = tempdir().unwrap();
        let archive = make_zip(&[("engine/readme.txt", b"ok", Some(0o100644))]);

        unzip_file(dir.path(), archive).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("engine/readme.txt")).unwrap(),
            "ok"
        );
    }

    #[test]
    fn extracts_valid_nested_tar_gz_directories() {
        let dir = tempdir().unwrap();
        let archive = gzip(make_tar_entry(
            "engines/stockfish/bin/stockfish",
            EntryType::Regular,
            b"uci",
        ));

        extract_tar_file(dir.path(), archive, true).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("engines/stockfish/bin/stockfish")).unwrap(),
            "uci"
        );
    }

    #[test]
    fn rejects_zip_parent_directory_traversal() {
        let dir = tempdir().unwrap();
        let archive = make_zip(&[("../outside.txt", b"bad", Some(0o100644))]);

        assert!(unzip_file(dir.path(), archive).is_err());
        assert!(!dir.path().parent().unwrap().join("outside.txt").exists());
    }

    #[test]
    fn rejects_tar_absolute_paths() {
        let dir = tempdir().unwrap();
        let archive = make_tar_entry("/tmp/pawn-appetit-owned", EntryType::Regular, b"bad");

        assert!(extract_tar_file(dir.path(), archive, false).is_err());
        assert!(!Path::new("/tmp/pawn-appetit-owned").exists());
    }

    #[test]
    fn rejects_nested_tar_traversal() {
        let dir = tempdir().unwrap();
        let archive = make_tar_entry("safe/../../outside.txt", EntryType::Regular, b"bad");

        assert!(extract_tar_file(dir.path(), archive, false).is_err());
        assert!(!dir.path().parent().unwrap().join("outside.txt").exists());
    }

    #[test]
    fn rejects_malicious_tar_symlink() {
        let dir = tempdir().unwrap();
        let archive = make_tar_link("engine-link", EntryType::Symlink, "/tmp");

        assert!(extract_tar_file(dir.path(), archive, false).is_err());
        assert!(!dir.path().join("engine-link").exists());
    }

    #[test]
    fn rejects_malicious_tar_hardlink() {
        let dir = tempdir().unwrap();
        let archive = make_tar_link("engine-hardlink", EntryType::Link, "../../outside");

        assert!(extract_tar_file(dir.path(), archive, false).is_err());
        assert!(!dir.path().join("engine-hardlink").exists());
    }

    #[test]
    fn ensure_inside_accepts_existing_and_new_nested_paths() {
        let base = tempdir().unwrap();
        let existing = base.path().join("engines");
        std::fs::create_dir(&existing).unwrap();

        assert!(ensure_inside(base.path(), &existing).is_ok());
        assert!(ensure_inside(base.path(), &base.path().join("a/b/c.bin")).is_ok());
    }

    #[test]
    fn ensure_inside_rejects_outside_and_parent_escape() {
        let base = tempdir().unwrap();
        let other = tempdir().unwrap();

        assert!(ensure_inside(base.path(), &other.path().join("x")).is_err());
        assert!(ensure_inside(base.path(), &base.path().join("a/../../x")).is_err());
        assert!(ensure_inside(base.path(), Path::new("relative/x")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn ensure_inside_rejects_symlink_pointing_out() {
        let base = tempdir().unwrap();
        let other = tempdir().unwrap();
        std::os::unix::fs::symlink(other.path(), base.path().join("link")).unwrap();
        std::os::unix::fs::symlink(other.path().join("gone"), base.path().join("dangling"))
            .unwrap();

        assert!(ensure_inside(base.path(), &base.path().join("link/x")).is_err());
        assert!(ensure_inside(base.path(), &base.path().join("dangling")).is_err());
    }

    #[tokio::test]
    async fn download_replaces_file_when_stream_completes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("engine.bin");
        std::fs::write(&path, b"old").unwrap();
        let chunks: Vec<Result<Vec<u8>, Error>> = vec![Ok(b"ne".to_vec()), Ok(b"w".to_vec())];

        download_atomic(stream::iter(chunks), &path, Some(3), |_| Ok(()))
            .await
            .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(!dir.path().join("engine.bin.part").exists());
    }

    #[tokio::test]
    async fn failed_download_keeps_old_file_and_leaves_no_part() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("engine.bin");
        std::fs::write(&path, b"old").unwrap();
        let chunks: Vec<Result<Vec<u8>, Error>> = vec![
            Ok(b"partial".to_vec()),
            Err(Error::PackageManager("connection reset".into())),
        ];

        let result = download_atomic(stream::iter(chunks), &path, None, |_| Ok(())).await;

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert!(!dir.path().join("engine.bin.part").exists());
    }

    #[test]
    fn extract_atomic_replaces_existing_top_level_entries() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("engine")).unwrap();
        std::fs::write(dir.path().join("engine/stale.txt"), b"stale").unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"keep").unwrap();
        let archive = make_zip(&[("engine/new.txt", b"new", Some(0o100644))]);

        extract_atomic(dir.path(), ArchiveKind::Zip, archive).unwrap();

        assert!(!dir.path().join("engine/stale.txt").exists());
        assert_eq!(
            std::fs::read(dir.path().join("engine/new.txt")).unwrap(),
            b"new"
        );
        assert_eq!(std::fs::read(dir.path().join("keep.txt")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn malicious_archive_leaves_target_unchanged() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("engine")).unwrap();
        std::fs::write(dir.path().join("engine/old.txt"), b"old").unwrap();
        let archive = make_zip(&[
            ("engine/new.txt", b"new", Some(0o100644)),
            ("../outside.txt", b"bad", Some(0o100644)),
        ]);

        assert!(extract_atomic(dir.path(), ArchiveKind::Zip, archive).is_err());

        assert_eq!(
            std::fs::read(dir.path().join("engine/old.txt")).unwrap(),
            b"old"
        );
        assert!(!dir.path().join("engine/new.txt").exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn corrupt_archive_leaves_target_unchanged() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("old.txt"), b"old").unwrap();

        assert!(extract_atomic(dir.path(), ArchiveKind::Zip, b"not a zip".to_vec()).is_err());

        assert_eq!(std::fs::read(dir.path().join("old.txt")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
