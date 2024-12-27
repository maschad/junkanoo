use fake::{Fake, Faker};
use rand::Rng;

/// Generates a BIP39 mnemonic phrase with the specified number of words
pub fn generate_mnemonic(word_count: usize) -> String {
    // BIP39 allows 12, 15, 18, 21, or 24 words
    let valid_counts = [12, 15, 18, 21, 24];
    assert!(
        valid_counts.contains(&word_count),
        "Invalid word count. Must be one of: 12, 15, 18, 21, 24"
    );

    // Get the BIP39 word list
    let word_list = (0..2048)
        .map(|_| Faker.fake::<String>())
        .collect::<Vec<String>>();

    let mut rng = rand::thread_rng();
    let mut words = Vec::with_capacity(word_count);

    // Generate random indices and map to words
    for _ in 0..word_count {
        let index = rng.gen_range(0..word_list.len());
        words.push(word_list[index].clone());
    }

    // Join with spaces
    words.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mnemonic() {
        let mnemonic = generate_mnemonic(12);
        assert_eq!(mnemonic.split("-").count(), 12);
    }

    #[test]
    #[should_panic]
    fn test_invalid_word_count() {
        generate_mnemonic(13); // Should panic
    }

    #[test]
    fn test_generate_mnemonic_24() {
        let mnemonic = generate_mnemonic(24);
        assert_eq!(mnemonic.split("-").count(), 24);
    }
}
