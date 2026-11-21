//! Originals crossing into or out of a browser page. An upload keeps its temporary files for the life of the page.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use octoii::Caller;
use octoii::protocol::{Error, Scope};
use octoii::threads::files::BYTES_MAX;

use crate::extension::failed;
use crate::protocol::{ChooseFiles, KeepDownload, KeptDownload};
use crate::{Browser, BrowserError};

#[derive(Debug)]
pub(crate) struct Upload {
    pub(crate) paths: Vec<PathBuf>,
    _directory: tempfile::TempDir,
}

impl Upload {
    pub(crate) fn stage(files: impl IntoIterator<Item = Result<(String, Bytes), BrowserError>>) -> Result<Self, BrowserError> {
        let directory = tempfile::Builder::new()
            .prefix("octoii-browser-upload-")
            .tempdir()
            .map_err(file_error)?;
        let mut paths = Vec::new();
        let mut remaining = BYTES_MAX;
        for (index, file) in files.into_iter().enumerate() {
            let (name, bytes) = file?;
            remaining = remaining.checked_sub(bytes.len()).ok_or_else(too_large)?;
            let folder = directory.path().join(index.to_string());
            std::fs::create_dir(&folder).map_err(file_error)?;
            let path = folder.join(basename(&name));
            std::fs::File::create_new(&path)
                .and_then(|mut file| file.write_all(&bytes))
                .map_err(file_error)?;
            paths.push(path);
        }
        Ok(Self {
            paths,
            _directory: directory,
        })
    }
}

fn basename(name: &str) -> &str {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or_default();
    if leaf.contains('\0') || (cfg!(windows) && leaf.contains(':')) {
        return "file";
    }
    Path::new(leaf).file_name().and_then(|name| name.to_str()).unwrap_or("file")
}

pub(crate) fn file_error(error: impl std::fmt::Display) -> BrowserError {
    BrowserError::FileTransfer(error.to_string())
}

fn too_large() -> BrowserError {
    BrowserError::FileTransfer(format!("the files hold at most {BYTES_MAX} bytes"))
}

/// The browser only receives files the person kept in this thread.
pub(crate) async fn choose(browser: &Browser, caller: &Caller, request: ChooseFiles) -> Result<(), Error> {
    if request.files.is_empty() {
        return browser
            .choose_files(&request.page, request.chooser, None)
            .await
            .map_err(|error| failed(&error));
    }
    caller.require_scope(Scope::ThreadsOperate)?;
    let view = browser.view(&request.page).await.ok_or_else(|| failed(&BrowserError::NoSuchPage))?;
    let volume = caller.volume();
    let transfer = tokio::task::spawn_blocking(move || {
        Upload::stage(request.files.into_iter().map(|pick| {
            let bytes = volume
                .read_file(&view.thread, &pick.reference)
                .map_err(file_error)?
                .ok_or_else(|| BrowserError::FileTransfer("the thread does not keep that file".into()))?;
            Ok((pick.filename.unwrap_or_else(|| "file".into()), bytes))
        }))
    })
    .await
    .map_err(|error| failed(&file_error(error)))?
    .map_err(|error| failed(&error))?;
    browser
        .choose_files(&request.page, request.chooser, Some(transfer))
        .await
        .map_err(|error| failed(&error))
}

/// A completed download is saved by its original bytes, under the thread that owned it.
pub(crate) async fn keep_download(browser: &Browser, caller: &Caller, request: KeepDownload) -> Result<KeptDownload, Error> {
    caller.require_scope(Scope::ThreadsOperate)?;
    let download = browser
        .saved_download(&request.page, &request.download)
        .await
        .ok_or_else(|| failed(&BrowserError::FileTransfer("the download is gone or has not finished".into())))?;
    let bytes = tokio::task::spawn_blocking(move || read_download(&download.path))
        .await
        .map_err(|error| failed(&file_error(error)))?
        .map_err(|error| failed(&error))?;
    caller
        .volume()
        .keep_file(&download.thread, bytes, basename(&download.name))
        .await
        .map(|(file, size)| KeptDownload { file, size })
        .map_err(|error| failed(&file_error(error)))
}

fn read_download(path: &Path) -> Result<Bytes, BrowserError> {
    let metadata = std::fs::symlink_metadata(path).map_err(file_error)?;
    if !metadata.is_file() {
        return Err(BrowserError::FileTransfer("the download is not a regular file".into()));
    }
    if metadata.len() > BYTES_MAX as u64 {
        return Err(too_large());
    }
    let input = std::fs::File::open(path).map_err(file_error)?;
    let mut bytes = Vec::new();
    input.take(BYTES_MAX as u64 + 1).read_to_end(&mut bytes).map_err(file_error)?;
    if bytes.len() > BYTES_MAX {
        return Err(too_large());
    }
    Ok(bytes.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_the_original_bytes_and_keeps_same_named_files_separate() {
        let upload = Upload::stage([
            Ok(("../../photo.txt".into(), Bytes::from_static(b"first"))),
            Ok(("other\\photo.txt".into(), Bytes::from_static(b"second"))),
        ])
        .expect("staged files");
        assert_ne!(upload.paths[0], upload.paths[1]);
        assert_eq!(upload.paths[0].file_name().and_then(|name| name.to_str()), Some("photo.txt"));
        assert_eq!(upload.paths[1].file_name().and_then(|name| name.to_str()), Some("photo.txt"));
        assert_eq!(std::fs::read(&upload.paths[0]).expect("first file"), b"first");
        assert_eq!(std::fs::read(&upload.paths[1]).expect("second file"), b"second");
    }
}
