//! Curated model catalog and downloads.
//!
//! Removes the manual `curl` step: the Models pane lists a small set of
//! whisper and Silero VAD models, downloads them to ~/.cache/pie/models with
//! progress, and selects one into settings. Custom paths still work — a
//! selected catalog model just writes its path into the same setting.

use std::path::PathBuf;

use serde::Serialize;

use crate::settings::Settings;

#[derive(Clone, Copy, PartialEq)]
pub enum ModelKind {
    Whisper,
    Vad,
}

impl ModelKind {
    fn as_str(self) -> &'static str {
        match self {
            ModelKind::Whisper => "whisper",
            ModelKind::Vad => "vad",
        }
    }
}

struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    kind: ModelKind,
    url: &'static str,
    filename: &'static str,
    size_mb: u32,
}

/// Whisper GGML models come from the whisper.cpp HuggingFace repo; the Silero
/// VAD ONNX from Handy's mirror. Both are the files used elsewhere in PIE.
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "whisper-tiny-en",
        name: "Whisper Tiny (English)",
        description: "Fastest, lowest accuracy. Good for testing.",
        kind: ModelKind::Whisper,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        filename: "ggml-tiny.en.bin",
        size_mb: 75,
    },
    CatalogEntry {
        id: "whisper-base-en",
        name: "Whisper Base (English)",
        description: "Balanced speed and accuracy.",
        kind: ModelKind::Whisper,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        filename: "ggml-base.en.bin",
        size_mb: 142,
    },
    CatalogEntry {
        id: "whisper-small-en",
        name: "Whisper Small (English)",
        description: "Most accurate English model here; slower.",
        kind: ModelKind::Whisper,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        filename: "ggml-small.en.bin",
        size_mb: 466,
    },
    CatalogEntry {
        id: "whisper-base",
        name: "Whisper Base (multilingual)",
        description: "Balanced, supports non-English languages.",
        kind: ModelKind::Whisper,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        filename: "ggml-base.bin",
        size_mb: 142,
    },
    CatalogEntry {
        id: "silero-vad",
        name: "Silero VAD v4",
        description: "Voice activity detection — trims silence.",
        kind: ModelKind::Vad,
        url: "https://blob.handy.computer/silero_vad_v4.onnx",
        filename: "silero_vad_v4.onnx",
        size_mb: 2,
    },
];

/// Where downloaded models live. Matches the CLI/engine default cache.
pub fn models_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache/pie/models")
}

fn find(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

/// One catalog row as seen by the frontend.
#[derive(Serialize)]
pub struct ModelInfo {
    id: String,
    name: String,
    description: String,
    kind: String,
    size_mb: u32,
    downloaded: bool,
    selected: bool,
    path: String,
}

pub fn list_models(settings: &Settings) -> Vec<ModelInfo> {
    CATALOG
        .iter()
        .map(|e| {
            let path = models_dir().join(e.filename);
            let path_str = path.to_string_lossy().into_owned();
            let selected_setting = match e.kind {
                ModelKind::Whisper => &settings.whisper_model,
                ModelKind::Vad => &settings.silero_model,
            };
            ModelInfo {
                id: e.id.to_string(),
                name: e.name.to_string(),
                description: e.description.to_string(),
                kind: e.kind.as_str().to_string(),
                size_mb: e.size_mb,
                downloaded: path.exists(),
                selected: Settings::expand(selected_setting) == path,
                path: path_str,
            }
        })
        .collect()
}

/// Resolve a catalog id to its (kind, url, destination path).
pub fn resolve(id: &str) -> Option<(ModelKind, &'static str, PathBuf)> {
    let entry = find(id)?;
    Some((entry.kind, entry.url, models_dir().join(entry.filename)))
}

/// Stream `url` to `dest`, calling `on_progress(received, total)` as bytes
/// arrive. Downloads to a sibling `.part` file and renames on success, so an
/// interrupted download never leaves a truncated model that looks complete.
pub async fn download_to(
    url: &str,
    dest: &std::path::Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<u64, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Can't create models folder: {e}"))?;
    }

    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response.content_length().unwrap_or(0);

    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("Can't write file: {e}"))?;

    let mut stream = response.bytes_stream();
    let mut received: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write failed: {e}"))?;
        received += chunk.len() as u64;
        on_progress(received, total);
    }
    file.flush().await.map_err(|e| format!("Flush failed: {e}"))?;
    drop(file);
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| format!("Can't finalize download: {e}"))?;
    Ok(received)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_resolve() {
        let mut seen = std::collections::HashSet::new();
        for e in CATALOG {
            assert!(seen.insert(e.id), "duplicate catalog id: {}", e.id);
            assert!(resolve(e.id).is_some());
        }
    }

    // Real network download of the smallest catalog model (~2 MB). Ignored by
    // default so offline `cargo test` stays green; run explicitly to verify:
    //   cargo test -p pie-desktop --ignored download_streams_to_file
    #[test]
    #[ignore]
    fn download_streams_to_file() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (_, url, _) = resolve("silero-vad").unwrap();
            let dir = std::env::temp_dir().join("pie-model-test");
            let dest = dir.join("silero_vad_v4.onnx");
            let _ = std::fs::remove_file(&dest);

            let mut last = 0u64;
            let received = download_to(url, &dest, |r, _t| last = r).await.unwrap();

            assert!(received > 1_000_000, "expected >1MB, got {received}");
            assert_eq!(last, received, "final progress must equal total received");
            let on_disk = std::fs::metadata(&dest).unwrap().len();
            assert_eq!(on_disk, received, "file size must match downloaded bytes");
            let _ = std::fs::remove_file(&dest);
        });
    }
}
