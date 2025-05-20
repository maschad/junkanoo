use crate::service::node::FileMetadata;
use async_std::{io, path::PathBuf};
use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p::Stream;
use tokio::io::AsyncReadExt as _;

pub struct FileTransfer {
    path: PathBuf,
    chunk_size: usize,
    progress: u64,
}

impl FileTransfer {
    pub fn new(metadata: FileMetadata) -> Self {
        Self {
            path: metadata.path.into(),
            chunk_size: 1024 * 1024, // 1MB chunks
            progress: 0,
        }
    }

    pub async fn stream_file(&mut self, stream: &mut Stream) -> io::Result<()> {
        let file = tokio::fs::File::open(&self.path).await?;
        let mut reader = tokio::io::BufReader::new(file);
        let mut buffer = vec![0; self.chunk_size];

        while let Ok(n) = reader.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            stream.write_all(&buffer[..n]).await?;
            self.progress += n as u64;
        }
        Ok(())
    }
}

pub struct FileReceiver {
    chunk_size: usize,
    progress: u64,
}

impl FileReceiver {
    pub fn new() -> Self {
        Self {
            chunk_size: 1024 * 1024, // 1MB chunks
            progress: 0,
        }
    }

    pub async fn receive_file(&mut self, stream: &mut Stream, file_name: String) -> io::Result<()> {
        let mut buffer = vec![0; self.chunk_size];
        let mut file_data = Vec::new();

        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => break, // End of stream
                Ok(n) => {
                    file_data.extend_from_slice(&buffer[..n]);
                    self.progress += n as u64;
                    tracing::debug!("Received {} bytes, total: {}", n, self.progress);
                }
                Err(e) => {
                    tracing::error!("Error reading from stream: {}", e);
                    return Err(e.into());
                }
            }
        }

        if !file_data.is_empty() {
            tokio::fs::write(&file_name, file_data).await?;
            tracing::info!("Successfully saved file as {}", file_name);
        }

        Ok(())
    }
}
