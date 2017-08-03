use bitreader::BitReader;
use bit_vec::BitVec;

use data_encoding::hex;

use ::crypto::{gen_random_bytes, sha256, pbkdf2};
use ::error::{Error, ErrorKind};
use ::keytype::KeyType;
use ::language::Language;
use ::util::bit_from_u16_as_u11;

#[derive(Debug)]
pub struct Mnemonic {
    pub string: String,
    pub seed: Vec<u8>,
    pub lang: Language
}

impl Mnemonic {

    /// Generates a new `Mnemonic` struct
    ///
    /// When returned, the struct will be filled in with the phrase and the seed value
    /// as 64 bytes raw
    ///
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, KeyType, Language};
    ///
    /// let kt = KeyType::for_word_length(12).unwrap();
    ///
    /// let bip39 = match Mnemonic::new(&kt, Language::English, "") {
    ///     Ok(b) => b,
    ///     Err(e) => { println!("e: {}", e); return }
    /// };
    ///
    /// let phrase = &bip39.string;
    /// let seed = &bip39.seed;
    /// println!("phrase: {}", string);
    /// ```
    pub fn new<S>(key_type: &KeyType, lang: Language, password: S) -> Result<Mnemonic, Error>  where S: Into<String> {

        let entropy_bits = key_type.entropy_bits();

        let num_words = key_type.word_length();

        let word_list = Language::get_wordlist(&lang);

        let entropy = try!(gen_random_bytes(entropy_bits / 8));


        let entropy_hash = sha256(entropy.as_ref());

        // we put both the entropy and the hash of the entropy (in that order) into a single vec
        // and then just read 11 bits at a time out of the entire thing `num_words` times. We
        // can do that because:
        //
        // 12 words * 11bits = 132bits
        // 15 words * 11bits = 165bits
        //
        // ... and so on. It grabs the entropy and then the right number of hash bits and no more.

        let mut combined = Vec::from(entropy);
        combined.extend(&entropy_hash);

        let mut reader = BitReader::new(combined.as_ref());

        let mut words: Vec<&str> = Vec::new();
        for _ in 0..num_words {
            let n = reader.read_u16(11);
            words.push(word_list[n.unwrap() as usize].as_ref());
        }

        let string = words.join(" ");

        Mnemonic::from_string(string, lang, password.into())
    }

    /// Create a `Mnemonic` struct from an existing mnemonic phrase
    ///
    /// The phrase supplied will be checked for word length and validated according to the checksum
    /// specified in BIP0039
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, KeyType, Language};
    ///
    /// let test_mnemonic = "park remain person kitchen mule spell knee armed position rail grid ankle";
    ///
    /// let b = Mnemonic::from_string(test_mnemonic, Language::English, "").unwrap();
    /// ```
    ///

    pub fn from_string<S>(string: S, lang: Language, password: S) -> Result<Mnemonic, Error> where S: Into<String> {
        let m = string.into();
        let p = password.into();
        try!(Mnemonic::validate(&*m, &lang));

        Ok(Mnemonic { string: (&m).clone(), seed: Mnemonic::generate_seed(&m.as_bytes(), &p), lang: lang})
    }

    /// Validate a mnemonic phrase
    ///
    /// The phrase supplied will be checked for word length and validated according to the checksum
    /// specified in BIP0039
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, KeyType, Language};
    ///
    /// let test_mnemonic = "park remain person kitchen mule spell knee armed position rail grid ankle";
    ///
    /// match Mnemonic::validate(test_mnemonic, &Language::English) {
    ///     Ok(_) => { println!("valid: {}", test_mnemonic); },
    ///     Err(e) => { println!("e: {}", e); return }
    /// }
    /// ```
    ///
    pub fn validate<S>(string: S, lang: &Language) -> Result<(), Error>  where S: Into<String> {

        Mnemonic::to_entropy(string, lang).and(Ok(()))
    }
    
    /// Convert mnemonic word list to original entropy value.
    ///
    /// The phrase supplied will be checked for word length and validated according to the checksum
    /// specified in BIP0039
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, KeyType, Language};
    ///
    /// let test_mnemonic = "park remain person kitchen mule spell knee armed position rail grid ankle";
    ///
    /// match Mnemonic::to_entropy(test_mnemonic, &Language::English) {
    ///     Ok(entropy) => { println!("valid, entropy is: {:?}", entropy); },
    ///     Err(e) => { println!("e: {}", e); return }
    /// }
    /// ```
    ///
    pub fn to_entropy<S>(string: S, lang: &Language) -> Result<Vec<u8>, Error>  where S: Into<String> {

        let m = string.into();

        let key_type = try!(KeyType::for_mnemonic(&*m));
        let entropy_bits = key_type.entropy_bits();
        let checksum_bits = key_type.checksum_bits();

		let word_map = Language::get_wordmap(lang);
		
        let mut to_validate: BitVec = BitVec::new();

        for word in m.split(" ").into_iter() {
            let n = match word_map.get(word) {
                Some(n) => n,
                None => return Err(ErrorKind::InvalidWord.into())
            };
            for i in 0..11 {
                let bit = bit_from_u16_as_u11(*n, i);
                to_validate.push(bit);
            }
        }

        let mut checksum_to_validate = BitVec::new();
        &checksum_to_validate.extend((&to_validate).into_iter().skip(entropy_bits).take(checksum_bits));
        assert!(checksum_to_validate.len() == checksum_bits, "invalid checksum size");

        let mut entropy_to_validate = BitVec::new();
        &entropy_to_validate.extend((&to_validate).into_iter().take(entropy_bits));
        assert!(entropy_to_validate.len() == entropy_bits, "invalid entropy size");
		
		let entropy = entropy_to_validate.to_bytes();
		
        let hash = sha256(entropy.as_ref());

        let entropy_hash_to_validate_bits = BitVec::from_bytes(hash.as_ref());


        let mut new_checksum = BitVec::new();
        &new_checksum.extend(entropy_hash_to_validate_bits.into_iter().take(checksum_bits));
        assert!(new_checksum.len() == checksum_bits, "invalid new checksum size");
        if !(new_checksum == checksum_to_validate) {
            return Err(ErrorKind::InvalidChecksum.into())
        }

        Ok(entropy)
    }

    pub fn to_hex(&self) -> String {

        let seed: &[u8] = self.seed.as_ref();
        let hex = hex::encode(seed);

        hex
    }
    
    pub fn to_entropy_hex(&self) -> String {

        let entropy = Mnemonic::to_entropy(self.string.as_str(), &self.lang).unwrap();
        let hex = hex::encode(entropy.as_slice());

        hex
    }

    fn generate_seed(entropy: &[u8], password: &str) -> Vec<u8> {

        let salt = format!("mnemonic{}", password);
        let seed = pbkdf2(entropy, salt);

        seed
    }
}