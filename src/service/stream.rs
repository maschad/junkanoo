use async_stream::stream;
use futures::{stream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::{collections::HashMap, path::PathBuf, time::SystemTime};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
};

// Extend FsEntry to include search-relevant metadata
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FsEntry {
    #[serde(flatten)]
    entry_type: FsEntryType,
    modified: SystemTime,
    created: SystemTime,
    hash: Option<String>, // For caching and verification
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FsEntryType {
    Directory {
        path: PathBuf,
        name: String,
        entry_count: usize,
    },
    File {
        path: PathBuf,
        name: String,
        size: u64,
        mime_type: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FileRequest {
    ListDirectory {
        path: PathBuf,
        use_cache: bool,
    },
    Preview {
        path: PathBuf,
        preview_size: usize,
        use_cache: bool,
    },
    Search {
        root_path: PathBuf,
        query: SearchQuery,
    },
    Download {
        path: PathBuf,
        resume_position: Option<u64>,
    },
    CancelDownload {
        path: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchQuery {
    term: String,
    file_types: Vec<String>,
    size_range: Option<(u64, u64)>,
    modified_after: Option<SystemTime>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FileResponse {
    DirectoryListing(Vec<FsEntry>),
    Preview {
        content: Vec<u8>,
        mime_type: String,
        cache_key: String,
    },
    FileChunk {
        chunk: Vec<u8>,
        offset: u64,
        is_last: bool,
        checksum: String,
    },
    SearchResults(Vec<FsEntry>),
    ProgressUpdate {
        bytes_transferred: u64,
        total_bytes: u64,
    },
    Error(String),
}

#[derive(Clone)]
pub struct FileService {
    cache: DirectoryCache,
    preview_cache: PreviewCache,
    active_downloads: HashMap<PathBuf, DownloadState>,
}

#[derive(Clone)]
struct DirectoryCache {
    entries: HashMap<PathBuf, (Vec<FsEntry>, SystemTime)>,
    max_age: std::time::Duration,
}

#[derive(Clone)]
struct PreviewCache {
    previews: HashMap<String, (Vec<u8>, SystemTime)>,
    max_size: usize,
}

#[derive(Clone)]
struct DownloadState {
    progress: u64,
    total: u64,
    cancel_signal: tokio::sync::watch::Sender<bool>,
}

impl FileService {
    pub fn new() -> Self {
        Self {
            cache: DirectoryCache {
                entries: HashMap::new(),
                max_age: std::time::Duration::from_secs(30),
            },
            preview_cache: PreviewCache {
                previews: HashMap::new(),
                max_size: 50 * 1024 * 1024, // 50MB cache limit
            },
            active_downloads: HashMap::new(),
        }
    }

    // Add this method
    async fn read_directory(&self, path: &PathBuf) -> std::io::Result<Vec<FsEntry>> {
        let mut entries = Vec::new();
        let mut dir = fs::read_dir(path).await?;

        while let Some(entry) = dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            let entry_count = if metadata.is_dir() {
                let mut count = 0;
                let mut dir_entries = fs::read_dir(&path).await?;
                while let Some(_) = dir_entries.next_entry().await? {
                    count += 1;
                }
                count
            } else {
                0
            };

            let entry_type = if metadata.is_dir() {
                FsEntryType::Directory {
                    path: path.clone(),
                    name,
                    entry_count,
                }
            } else {
                FsEntryType::File {
                    path: path.clone(),
                    name,
                    size: metadata.len(),
                    mime_type: mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .to_string(),
                }
            };

            entries.push(FsEntry {
                entry_type,
                modified: metadata.modified()?,
                created: metadata.created()?,
                hash: None,
            });
        }

        Ok(entries)
    }

    async fn handle_request(
        &mut self,
        request: FileRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = FileResponse> + Send>>, std::io::Error> {
        match request {
            FileRequest::ListDirectory { path, use_cache } => {
                if use_cache {
                    if let Some((entries, cache_time)) = self.cache.entries.get(&path) {
                        if cache_time.elapsed().unwrap() < self.cache.max_age {
                            return Ok(Box::pin(stream::iter(vec![
                                FileResponse::DirectoryListing(entries.clone()),
                            ])));
                        }
                    }
                }

                let entries = self.read_directory(&path).await?;
                self.cache
                    .entries
                    .insert(path, (entries.clone(), SystemTime::now()));
                Ok(Box::pin(stream::iter(vec![
                    FileResponse::DirectoryListing(entries),
                ])))
            }

            FileRequest::Search { root_path, query } => {
                let stream = self.search_files(root_path, query).await?;
                Ok(Box::pin(stream))
            }

            FileRequest::Preview {
                path,
                preview_size,
                use_cache,
            } => {
                let cache_key = format!("{}:{}", path.display(), preview_size);

                if use_cache {
                    if let Some((preview, _)) = self.preview_cache.previews.get(&cache_key) {
                        return Ok(Box::pin(stream::iter(vec![FileResponse::Preview {
                            content: preview.clone(),
                            mime_type: mime_guess::from_path(&path)
                                .first_or_octet_stream()
                                .to_string(),
                            cache_key,
                        }])));
                    }
                }

                let preview = self.generate_preview(&path, preview_size).await?;
                self.preview_cache
                    .previews
                    .insert(cache_key.clone(), (preview.clone(), SystemTime::now()));

                Ok(Box::pin(stream::iter(vec![FileResponse::Preview {
                    content: preview,
                    mime_type: mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .to_string(),
                    cache_key,
                }])))
            }

            FileRequest::Download {
                path,
                resume_position,
            } => {
                let (progress_tx, _) = tokio::sync::watch::channel(false);
                let state = DownloadState {
                    progress: resume_position.unwrap_or(0),
                    total: fs::metadata(&path).await?.len(),
                    cancel_signal: progress_tx,
                };

                self.active_downloads.insert(path.clone(), state);

                Ok(self.stream_file(path, resume_position).await?)
            }

            FileRequest::CancelDownload { path } => {
                if let Some(state) = self.active_downloads.get(&path) {
                    let _ = state.cancel_signal.send(true);
                }
                self.active_downloads.remove(&path);
                Ok(Box::pin(stream::iter(vec![])))
            }
        }
    }

    async fn search_files(
        &self,
        root: PathBuf,
        query: SearchQuery,
    ) -> Result<Pin<Box<dyn Stream<Item = FileResponse> + Send + 'static>>, std::io::Error> {
        let root = root.clone();
        let query = query.clone();
        let this = self.clone();

        Ok(Box::pin(stream! {
            let mut walker = async_walkdir::WalkDir::new(root);
            while let Some(entry_result) = walker.next().await {
                if let Ok(entry) = entry_result {
                    if let Ok(true) = this.matches_search(&entry, &query).await {
                        if let Ok(fs_entry) = this.entry_to_fs_entry(entry).await {
                            yield FileResponse::SearchResults(vec![fs_entry]);
                        }
                    }
                }
            }
        }))
    }

    async fn stream_file(
        &self,
        path: PathBuf,
        start_pos: Option<u64>,
    ) -> Result<Pin<Box<dyn Stream<Item = FileResponse> + Send>>, std::io::Error> {
        Ok(Box::pin(stream! {
            let mut file = match fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    yield FileResponse::Error(e.to_string());
                    return;
                }
            };
            let metadata = match file.metadata().await {
                Ok(m) => m,
                Err(e) => {
                    yield FileResponse::Error(e.to_string());
                    return;
                }
            };
            let total_size = metadata.len();
            let mut position = start_pos.unwrap_or(0);

            if let Some(pos) = start_pos {
                match file.seek(std::io::SeekFrom::Start(pos)).await {
                    Ok(_) => (),
                    Err(e) => {
                        yield FileResponse::Error(e.to_string());
                        return;
                    }
                }
            }

            let chunk_size = 64 * 1024; // 64KB chunks
            let mut buffer = vec![0u8; chunk_size];

            while position < total_size {
                let bytes_read = match file.read(&mut buffer).await {
                    Ok(n) => n,
                    Err(e) => {
                        yield FileResponse::Error(e.to_string());
                        return;
                    }
                };

                if bytes_read == 0 {
                    break;
                }

                let chunk = buffer[..bytes_read].to_vec();
                let checksum = calculate_checksum(&chunk);
                position += bytes_read as u64;

                yield FileResponse::FileChunk {
                    chunk,
                    offset: position - bytes_read as u64,
                    is_last: position >= total_size,
                    checksum,
                };

                yield FileResponse::ProgressUpdate {
                    bytes_transferred: position,
                    total_bytes: total_size,
                };
            }
        }))
    }

    // Helper methods...
    async fn matches_search(
        &self,
        entry: &async_walkdir::DirEntry,
        query: &SearchQuery,
    ) -> Result<bool, std::io::Error> {
        let metadata = entry.metadata().await?;
        let name = entry.file_name().to_string_lossy().to_lowercase();

        // Check name match
        if !name.contains(&query.term.to_lowercase()) {
            return Ok(false);
        }

        // Check file type
        if !query.file_types.is_empty() {
            if let Some(ext) = entry.path().extension() {
                if !query
                    .file_types
                    .contains(&ext.to_string_lossy().to_string())
                {
                    return Ok(false);
                }
            }
        }

        // Check size range
        if let Some((min, max)) = query.size_range {
            let size = metadata.len();
            if size < min || size > max {
                return Ok(false);
            }
        }

        // Check modification time
        if let Some(modified_after) = query.modified_after {
            if metadata.modified()? < modified_after {
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn generate_preview(&self, path: &PathBuf, size: usize) -> std::io::Result<Vec<u8>> {
        let mut file = fs::File::open(path).await?;
        let mut preview = vec![0u8; size];
        let n = file.read(&mut preview).await?;
        preview.truncate(n);
        Ok(preview)
    }

    async fn entry_to_fs_entry(&self, entry: async_walkdir::DirEntry) -> std::io::Result<FsEntry> {
        let metadata = entry.metadata().await?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();

        let entry_type = if metadata.is_dir() {
            let mut count = 0;
            let mut dir_entries = fs::read_dir(&path).await?;
            while let Some(_) = dir_entries.next_entry().await? {
                count += 1;
            }
            FsEntryType::Directory {
                path: path.clone(),
                name,
                entry_count: count,
            }
        } else {
            FsEntryType::File {
                path: path.clone(),
                name,
                size: metadata.len(),
                mime_type: mime_guess::from_path(&path)
                    .first_or_octet_stream()
                    .to_string(),
            }
        };

        Ok(FsEntry {
            entry_type,
            modified: metadata.modified()?,
            created: metadata.created()?,
            hash: None,
        })
    }
}

fn calculate_checksum(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
