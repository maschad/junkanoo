use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncWrite};

const CHUNK_SIZE: usize = 1024 * 64; // 64KB chunks

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    ListDirectory { path: PathBuf },
    GetFile { path: PathBuf },
    StartFileTransfer { path: PathBuf },
    FileChunk { 
        sequence: u32,
        data: Vec<u8>,
        is_last: bool 
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    DirectoryListing {
        entries: Vec<FileEntry>,
    },
    FileMetadata {
        path: PathBuf,
        size: u64,
        is_dir: bool,
    },
    ReadyToReceive,
    ChunkReceived {
        sequence: u32,
        status: TransferStatus,
    },
    Error(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum TransferStatus {
    InProgress { bytes_transferred: u64, total_bytes: u64 },
    Complete,
    Failed(String),
}

pub struct FileTransferProtocol<S> {
    stream: S,
    transfer_state: Option<TransferState>,
}

struct TransferState {
    path: PathBuf,
    size: u64,
    bytes_transferred: u64,
    current_chunk: u32,
}

impl<S> FileTransferProtocol<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            transfer_state: None,
        }
    }

    pub async fn send_request(&mut self, request: Request) -> Result<()> {
        let msg = serde_json::to_vec(&request)?;
        // Send message length as u32 first
        let len = msg.len() as u32;
        let len_bytes = len.to_be_bytes();
        tokio::io::AsyncWriteExt::write_all(&mut self.stream, &len_bytes).await?;
        tokio::io::AsyncWriteExt::write_all(&mut self.stream, &msg).await?;
        Ok(())
    }

    pub async fn receive_response(&mut self) -> Result<Response> {
        // Read message length
        let mut len_bytes = [0u8; 4];
        tokio::io::AsyncReadExt::read_exact(&mut self.stream, &mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Read the actual message
        let mut buffer = vec![0u8; len];
        tokio::io::AsyncReadExt::read_exact(&mut self.stream, &mut buffer).await?;
        
        let response: Response = serde_json::from_slice(&buffer)?;
        Ok(response)
    }

    pub async fn send_file(&mut self, path: PathBuf, size: u64) -> Result<()> {
        self.transfer_state = Some(TransferState {
            path: path.clone(),
            size,
            bytes_transferred: 0,
            current_chunk: 0,
        });

        let request = Request::StartFileTransfer { path };
        self.send_request(request).await?;

        // Wait for ready response
        match self.receive_response().await? {
            Response::ReadyToReceive => Ok(()),
            Response::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn send_chunk(&mut self, data: Vec<u8>, is_last: bool) -> Result<TransferStatus> {
        let state = self.transfer_state.as_mut()
            .ok_or_else(|| anyhow::anyhow!("No active transfer"))?;

        let request = Request::FileChunk {
            sequence: state.current_chunk,
            data,
            is_last,
        };

        self.send_request(request).await?;

        match self.receive_response().await? {
            Response::ChunkReceived { status, .. } => Ok(status),
            Response::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn receive_file(&mut self) -> Result<(PathBuf, Vec<u8>)> {
        match self.receive_response().await? {
            Response::FileMetadata { path, size, .. } => {
                self.transfer_state = Some(TransferState {
                    path: path.clone(),
                    size,
                    bytes_transferred: 0,
                    current_chunk: 0,
                });

                // Send ready signal
                self.send_request(Request::StartFileTransfer { path: path.clone() }).await?;

                let mut file_data = Vec::with_capacity(size as usize);
                let mut current_chunk = 0;

                loop {
                    match self.receive_response().await? {
                        Response::FileChunk { sequence, data, is_last } => {
                            if sequence != current_chunk {
                                return Err(anyhow::anyhow!("Chunk sequence mismatch"));
                            }

                            file_data.extend_from_slice(&data);
                            current_chunk += 1;

                            if is_last {
                                break;
                            }
                        }
                        Response::Error(e) => return Err(anyhow::anyhow!(e)),
                        _ => return Err(anyhow::anyhow!("Unexpected response")),
                    }
                }

                Ok((path, file_data))
            }
            Response::Error(e) => Err(anyhow::anyhow!(e)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}

