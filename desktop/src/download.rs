//! Downloads a model. It streams to disk, reports progress, and only installs a complete file.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const GGML_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

/// Returns `Ok(false)` if cancelled. A partial file never reaches `target`.
pub fn fetch(
    url: &str,
    target: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<bool, String> {
    let response = ureq::get(url)
        .call()
        .map_err(|_| "Model server returned an error. Please retry.".to_string())?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let folder = target.parent().ok_or("The model folder is missing.")?;
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let partial = target.with_extension("part");
    let result = save(
        response.into_body().into_reader(),
        &partial,
        total,
        cancel,
        &mut progress,
    );
    match result {
        Ok(true) => fs::rename(&partial, target)
            .map(|()| true)
            .map_err(|e| e.to_string()),
        other => {
            let _ = fs::remove_file(&partial);
            other
        }
    }
}

fn save(
    mut body: impl Read,
    path: &Path,
    total: u64,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(f64),
) -> Result<bool, String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1 << 20];
    let mut written = 0_u64;
    let mut checked = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let count = body.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if !checked {
            if count < GGML_MAGIC.len() || buffer[..GGML_MAGIC.len()] != GGML_MAGIC {
                return Err("Downloaded file is not a GGML Whisper model.".into());
            }
            checked = true;
        }
        file.write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        written += count as u64;
        if total > 0 {
            progress(written as f64 / total as f64);
        }
    }
    if !checked || (total > 0 && written != total) {
        return Err("The download ended early. Please retry.".into());
    }
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Serves one response on a local port and returns its address.
    fn serve(status: &str, length: usize, body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let head =
            format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        });
        format!("http://{address}/model.bin")
    }
    fn folder(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("soundveil-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }
    fn model(size: usize) -> Vec<u8> {
        let mut data = GGML_MAGIC.to_vec();
        data.resize(size, 7);
        data
    }

    #[test]
    fn installs_a_complete_model() {
        let target = folder("complete").join("model.bin");
        let data = model(3_000_000);
        let url = serve("200 OK", data.len(), data.clone());
        let mut last = 0.0;
        let done = fetch(&url, &target, &AtomicBool::new(false), |value| last = value);
        assert_eq!(done, Ok(true));
        assert_eq!(fs::read(&target).unwrap(), data);
        assert_eq!(last, 1.0);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_model() {
        let target = folder("wrong").join("model.bin");
        let url = serve("200 OK", 8, b"<html>  ".to_vec());
        assert!(fetch(&url, &target, &AtomicBool::new(false), |_| {}).is_err());
        assert!(!target.exists());
        assert!(!target.with_extension("part").exists());
    }

    #[test]
    fn rejects_a_short_download() {
        let target = folder("short").join("model.bin");
        let url = serve("200 OK", 1000, model(10));
        assert!(fetch(&url, &target, &AtomicBool::new(false), |_| {}).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn reports_a_server_error() {
        let target = folder("error").join("model.bin");
        let url = serve("404 Not Found", 0, Vec::new());
        assert!(fetch(&url, &target, &AtomicBool::new(false), |_| {}).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn cancelling_leaves_nothing_behind() {
        let target = folder("cancel").join("model.bin");
        let data = model(1000);
        let url = serve("200 OK", data.len(), data);
        assert_eq!(
            fetch(&url, &target, &AtomicBool::new(true), |_| {}),
            Ok(false)
        );
        assert!(!target.exists());
        assert!(!target.with_extension("part").exists());
    }
}
