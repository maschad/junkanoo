use anyhow::{anyhow, Result};
use bip39::Language;
use bs58;
use libp2p::PeerId;
use sha2::{Digest, Sha256};

pub struct PeerIDConverter {
    language: Language,
}

impl PeerIDConverter {
    pub fn new() -> Self {
        Self {
            language: Language::English,
        }
    }

    /// Convert a libp2p PeerID to BIP39 mnemonic words
    pub fn peer_id_to_mnemonic(&self, peer_id: &PeerId) -> Result<String> {
        // Decode base58 PeerID
        let bytes = bs58::decode(peer_id.to_string())
            .into_vec()
            .map_err(|e| anyhow!("Failed to decode base58: {}", e))?;

        // Verify multihash prefix (0x12, 0x20 for SHA2-256)
        if bytes.len() < 2 || bytes[0] != 0x12 || bytes[1] != 0x20 {
            return Err(anyhow!("Invalid multihash prefix"));
        }

        // Remove multihash prefix
        let hash_bytes = &bytes[2..];

        // Convert bytes to bits
        let bits = bytes_to_bits(hash_bytes);

        // Split into 11-bit chunks
        let chunks = bits
            .chunks(11)
            .map(|chunk| {
                let mut padded = [false; 11];
                padded[..chunk.len()].copy_from_slice(chunk);
                bits_to_index(&padded)
            })
            .collect::<Vec<_>>();

        // Get word list
        let word_list = self.language.word_list();

        // Convert indices to words
        let mut words = chunks
            .iter()
            .map(|&idx| word_list[idx as usize])
            .collect::<Vec<_>>();

        // Calculate checksum
        let checksum = Sha256::digest(hash_bytes);
        let checksum_index = (checksum[0] & 0x0F) as usize; // Use lower 4 bits
        words.push(word_list[checksum_index]);

        Ok(words.join("-"))
    }

    /// Convert a BIP39 mnemonic back to a libp2p PeerID
    pub fn mnemonic_to_peer_id(&self, mnemonic: &str) -> Result<String> {
        let words: Vec<&str> = mnemonic.split('-').collect();

        // Verify we have enough words
        if words.len() < 2 {
            return Err(anyhow!("Not enough mnemonic words"));
        }

        // Remove checksum word
        let checksum_word = *words.last().unwrap();
        let main_words = &words[..words.len() - 1];

        // Convert words to indices
        let word_list = self.language.word_list();
        let indices = main_words
            .iter()
            .map(|&word| {
                word_list
                    .iter()
                    .position(|&w| w == word)
                    .ok_or_else(|| anyhow!("Invalid word in mnemonic"))
            })
            .collect::<Result<Vec<_>>>()?;

        // Convert indices back to bytes
        let mut bits = Vec::new();
        for idx in indices {
            bits.extend(index_to_bits(idx as u16));
        }

        let hash_bytes = bits_to_bytes(&bits);

        // Calculate checksum
        let checksum = Sha256::digest(&hash_bytes);
        let expected_checksum_index = (checksum[0] & 0x0F) as usize; // Use lower 4 bits
        let expected_checksum_word = word_list[expected_checksum_index];

        if expected_checksum_word != checksum_word {
            return Err(anyhow!(
                "Checksum mismatch. Expected '{}', got '{}'",
                expected_checksum_word,
                checksum_word
            ));
        }

        // Add multihash prefix
        let mut full_bytes = vec![0x12, 0x20];
        full_bytes.extend(hash_bytes);

        Ok(bs58::encode(full_bytes).into_string())
    }
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &byte in bytes {
        // Process bits LSB first to match BIP39 spec
        for i in 0..8 {
            bits.push((byte & (1 << i)) != 0);
        }
    }
    bits
}

fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        // Process bits LSB first to match bytes_to_bits
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                byte |= 1 << i;
            }
        }
        bytes.push(byte);
    }
    bytes
}

fn bits_to_index(bits: &[bool]) -> u16 {
    // Process bits LSB first
    bits.iter()
        .take(11)
        .enumerate()
        .fold(0, |acc, (i, &bit)| acc | ((bit as u16) << i))
}

fn index_to_bits(index: u16) -> Vec<bool> {
    let mut bits = vec![false; 11];
    // Process bits LSB first
    for i in 0..11 {
        bits[i] = (index & (1 << i)) != 0;
    }
    bits
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_conversion() {
        let converter = PeerIDConverter::new();
        let peer_id = "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN";
        let peer_id = PeerId::from_str(peer_id).unwrap();

        // Print debug info
        let mnemonic = converter.peer_id_to_mnemonic(&peer_id).unwrap();
        println!("Generated mnemonic: {}", mnemonic);

        let recovered = converter.mnemonic_to_peer_id(&mnemonic).unwrap();
        println!("Original peer ID: {}", peer_id);
        println!("Recovered peer ID: {}", recovered);

        assert_eq!(peer_id.to_string(), recovered);
    }

    #[test]
    fn test_conversion_debug() {
        let converter = PeerIDConverter::new();
        let peer_id = "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN";
        let peer_id = PeerId::from_str(peer_id).unwrap();

        // Get the original bytes
        let original_bytes = bs58::decode(peer_id.to_string()).into_vec().unwrap();
        println!("Original bytes: {:?}", original_bytes);

        // Print the hash bytes (without prefix)
        let hash_bytes = &original_bytes[2..];
        println!("Hash bytes: {:?}", hash_bytes);

        // Calculate and print checksum directly
        let checksum = Sha256::digest(hash_bytes);
        println!("Checksum first byte: {:08b}", checksum[0]);
        println!("Checksum bits (top 4): {:04b}", checksum[0] >> 4);

        // Get and print bits
        let bits = bytes_to_bits(hash_bytes);
        println!("First 32 bits: {:?}", &bits[..32]);

        // Print first few 11-bit chunks
        let chunks: Vec<u16> = bits
            .chunks(11)
            .map(|chunk| {
                let mut padded = [false; 11];
                padded[..chunk.len()].copy_from_slice(chunk);
                bits_to_index(&padded)
            })
            .collect();
        println!("First few 11-bit chunks: {:?}", &chunks[..3]);

        // Get the mnemonic
        let mnemonic = converter.peer_id_to_mnemonic(&peer_id).unwrap();
        println!("Mnemonic: {}", mnemonic);

        // Now let's analyze the reverse process
        let words: Vec<&str> = mnemonic.split('-').collect(); // Changed to use hyphen
        let checksum_word = words.last().unwrap();
        println!("Checksum word: {}", checksum_word);

        // Convert main words back to indices
        let word_list = converter.language.word_list();
        let indices: Vec<_> = words[..words.len() - 1]
            .iter()
            .map(|&word| word_list.iter().position(|&w| w == word).unwrap())
            .collect();
        println!("First few indices from words: {:?}", &indices[..3]);

        // Convert indices back to bits and then to bytes
        let mut reverse_bits = Vec::new();
        for idx in &indices[..3] {
            let bits = index_to_bits(*idx as u16);
            println!("Index {} gives bits: {:?}", idx, bits);
            reverse_bits.extend(bits);
        }

        println!("First 32 reverse bits: {:?}", &reverse_bits[..32]);
    }

    #[test]
    fn test_checksum_calculation() {
        let hash_bytes = [
            6u8, 179, 96, 138, 160, 0, 39, 64, 73, 235, 40, 173, 142, 121, 58, 38, 255, 111, 171,
            40, 26, 125, 59, 215, 124, 209, 142, 183, 69, 223, 170, 187,
        ];

        let checksum = Sha256::digest(&hash_bytes);
        let checksum_index = checksum[0] & 0x0F;

        println!("Checksum byte: {:02x}", checksum[0]);
        println!("Checksum index: {}", checksum_index);

        let word_list = Language::English.word_list();
        let checksum_word = word_list[checksum_index as usize];
        println!("Checksum word: {}", checksum_word);
    }

    #[test]
    fn test_bit_operations() {
        let converter = PeerIDConverter::new();
        let peer_id = "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN";
        let peer_id = PeerId::from_str(peer_id).unwrap();

        // Forward conversion
        let bytes = bs58::decode(peer_id.to_string()).into_vec().unwrap();
        let hash_bytes = &bytes[2..];
        println!("\nForward conversion:");
        println!("Hash bytes (first few): {:?}", &hash_bytes[..4]);

        let bits = bytes_to_bits(hash_bytes);
        println!("First 16 bits: {:?}", &bits[..16]);

        let first_chunk = &bits[..11];
        let first_index = bits_to_index(first_chunk);
        println!("First index: {}", first_index);

        // Reverse conversion
        println!("\nReverse conversion:");
        let reverse_bits = index_to_bits(first_index);
        println!("Reversed bits: {:?}", reverse_bits);
        println!("Original bits: {:?}", first_chunk);

        // Compare
        assert_eq!(first_chunk, &reverse_bits[..11]);
    }
}
