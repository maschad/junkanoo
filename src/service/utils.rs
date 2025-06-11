use async_std::path::Path;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use libp2p::Stream;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt as TokioAsyncReadExt;
use tokio::io::AsyncWriteExt as TokioAsyncWriteExt;

#[derive(Debug)]
pub enum FileTransferError {
    Io(io::Error),
    Utf8(std::string::FromUtf8Error),
}

impl std::fmt::Display for FileTransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Utf8(e) => write!(f, "UTF-8 error: {e}"),
        }
    }
}

impl Error for FileTransferError {}

impl From<std::string::FromUtf8Error> for FileTransferError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::Utf8(err)
    }
}

impl From<FileTransferError> for Box<dyn Error + Send> {
    fn from(err: FileTransferError) -> Self {
        Box::new(err)
    }
}

pub struct FileTransfer {
    path: PathBuf,
    chunk_size: usize,
    progress: Arc<AtomicUsize>,
}

impl FileTransfer {
    pub fn new(path: &PathBuf) -> Self {
        // Convert to relative path immediately
        let current_dir = std::env::current_dir().unwrap_or_default();
        let relative_path = path
            .strip_prefix(&current_dir)
            .unwrap_or(path)
            .to_path_buf();

        tracing::debug!(
            "Relative path being used for file transfer: {:?}",
            relative_path
        );

        Self {
            path: relative_path,
            chunk_size: 1024 * 1024, // 1MB chunks
            progress: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn stream_file(&self, stream: &mut Stream) -> Result<(), Box<dyn Error + Send>> {
        let current_dir =
            std::env::current_dir().map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let full_path = current_dir.join(&self.path);

        tracing::debug!("Full path being used for file transfer: {:?}", full_path);

        let file = File::open(&full_path)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let metadata = file
            .metadata()
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let file_size =
            usize::try_from(metadata.len()).map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;

        // Send the relative path and file size
        let path_str = self.path.to_string_lossy().to_string();
        let path_bytes = path_str.as_bytes();
        stream
            .write_all(&(path_bytes.len() as u64).to_le_bytes())
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        stream
            .write_all(path_bytes)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        stream
            .write_all(&file_size.to_le_bytes())
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;

        let mut reader = tokio::io::BufReader::with_capacity(self.chunk_size, file);
        let mut buffer = vec![0u8; self.chunk_size];
        let mut total_read = 0;

        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
            if bytes_read == 0 {
                break;
            }

            stream
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
            total_read += bytes_read;
            self.progress.store(total_read, Ordering::SeqCst);
        }

        stream
            .flush()
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        Ok(())
    }
}

pub struct FileReceiver {
    chunk_size: usize,
    progress: Arc<AtomicUsize>,
}

impl FileReceiver {
    pub fn new() -> Self {
        Self {
            chunk_size: 1024 * 1024, // 1MB chunks
            progress: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn receive_file(
        &mut self,
        stream: &mut Stream,
    ) -> Result<String, Box<dyn Error + Send>> {
        tracing::debug!("Receiving file");

        // Read the relative path length
        let mut path_len_bytes = [0u8; 8];
        stream
            .read_exact(&mut path_len_bytes)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let path_len = u64::from_le_bytes(path_len_bytes) as usize;

        tracing::debug!("Path length: {}", path_len);

        // Read the relative path
        let mut path_bytes = vec![0u8; path_len];
        stream
            .read_exact(&mut path_bytes)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let relative_path = String::from_utf8(path_bytes)
            .map_err(|e| Box::new(FileTransferError::from(e)) as Box<dyn Error + Send>)?;

        // Read the file size
        let mut size_bytes = [0u8; 8];
        stream
            .read_exact(&mut size_bytes)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let file_size = usize::try_from(u64::from_le_bytes(size_bytes))
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        tracing::debug!("File size: {}", file_size);

        // Create the full save path by joining with current directory
        let current_dir =
            std::env::current_dir().map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let save_path = current_dir.join(&relative_path);
        tracing::debug!("Creating file at save path: {:?}", save_path);

        // Create parent directories if they don't exist
        if let Some(parent) = save_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        }

        // Create the file and write the contents
        tracing::debug!("Creating file");

        let mut file = File::create(&save_path)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        let mut buffer = vec![0u8; self.chunk_size];
        let mut total_read = 0;

        while total_read < file_size {
            let bytes_to_read = std::cmp::min(self.chunk_size, file_size - total_read);
            let bytes_read = stream
                .read(&mut buffer[..bytes_to_read])
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
            if bytes_read == 0 {
                break;
            }

            file.write_all(&buffer[..bytes_read])
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
            total_read += bytes_read;
            self.progress.store(total_read, Ordering::SeqCst);
        }

        file.flush()
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
        Ok(relative_path)
    }
}
