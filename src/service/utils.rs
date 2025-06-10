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
        // Use the absolute path directly
        let full_path = std::path::PathBuf::from(&self.path);
        tracing::info!("Attempting to stream file from path: {:?}", full_path);

        // First send the filename
        let file_name = full_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown_file");
        let file_name_bytes = file_name.as_bytes();
        let name_len = u32::try_from(file_name_bytes.len()).unwrap();

        // Get file size
        let file_size = tokio::fs::metadata(&full_path).await?.len();
        tracing::info!("File size: {} bytes", file_size);

        // Send the length of the filename first
        stream.write_all(&name_len.to_be_bytes()).await?;
        // Then send the filename
        stream.write_all(file_name_bytes).await?;
        // Send file size
        stream.write_all(&file_size.to_be_bytes()).await?;

        // Now send the file contents
        let file = tokio::fs::File::open(&full_path).await?;
        let mut reader = tokio::io::BufReader::new(file);
        let mut buffer = vec![0; self.chunk_size];

        while let Ok(n) = reader.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            stream.write_all(&buffer[..n]).await?;
            self.progress += n as u64;
        }

        // Send end of file marker
        stream.write_all(&[0u8; 4]).await?;

        tracing::info!("File sent successfully to peer, saved as: {:?}", self.path);
        Ok(())
    }
}

pub struct FileReceiver {
    chunk_size: usize,
    progress: u64,
}

impl FileReceiver {
    pub const fn new() -> Self {
        Self {
            chunk_size: 1024 * 1024, // 1MB chunks
            progress: 0,
        }
    }

    pub async fn receive_file(&mut self, stream: &mut Stream) -> io::Result<String> {
        // First read the filename length
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await?;
        let name_len = u32::from_be_bytes(len_bytes) as usize;

        // Then read the filename
        let mut file_name_bytes = vec![0u8; name_len];
        stream.read_exact(&mut file_name_bytes).await?;
        let file_name = String::from_utf8(file_name_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Read file size
        let mut size_bytes = [0u8; 8];
        stream.read_exact(&mut size_bytes).await?;
        let file_size = u64::from_be_bytes(size_bytes);

        // Now read the file contents
        let mut file_data = Vec::with_capacity(
            usize::try_from(file_size)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
        );
        let mut bytes_received = 0;

        #[allow(clippy::cast_possible_truncation)]
        while bytes_received < file_size {
            let bytes_to_read =
                std::cmp::min(self.chunk_size, (file_size - bytes_received) as usize);
            let mut chunk = vec![0; bytes_to_read];
            stream.read_exact(&mut chunk).await?;
            file_data.extend_from_slice(&chunk);
            bytes_received += bytes_to_read as u64;
            self.progress += bytes_to_read as u64;
            tracing::debug!("Received {} bytes, total: {}", bytes_to_read, self.progress);
        }

        if !file_data.is_empty() {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let unique_filename = format!("{timestamp}_{file_name}");
            tokio::fs::write(&unique_filename, file_data).await?;
            tracing::info!("Successfully saved file as {unique_filename}");
        }

        Ok(file_name)
    }
}
