use crate::service::node::FileMetadata;
use async_std::{io, path::PathBuf};
use futures::AsyncWriteExt;
use libp2p::Stream;
use tokio::io::AsyncReadExt;

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
