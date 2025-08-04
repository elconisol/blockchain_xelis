use std::borrow::Cow;

use xelis_common::{crypto::{hash, Hash, HASH_SIZE}, serializer::Serializer};

// This builder is used to build a merkle tree from a list of hashes
// It uses a bottom-up approach to build the tree
// The tree is built by taking pairs of hashes and hashing them together
// The resulting hash is then added to the list of hashes
// This process is repeated until there is only one hash left
pub struct MerkleBuilder<'a> {
    hashes: Vec<Cow<'a, Hash>>
}

impl<'a> MerkleBuilder<'a> {
    // Create a new MerkleBuilder
    pub fn new() -> Self {
        MerkleBuilder {
            hashes: Vec::new()
        }
    }

    // Create a new MerkleBuilder with a given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        MerkleBuilder {
            hashes: Vec::with_capacity(capacity)
        }
    }

    // Create a new MerkleBuilder from an iterator of hashes
    pub fn from_iter<I>(iter: I) -> Self
        where I: IntoIterator<Item = &'a Hash>
    {
        MerkleBuilder {
            hashes: iter.into_iter().map(|hash| Cow::Borrowed(hash)).collect()
        }
    }

   impl<'a> HashAccumulator<'a> {
    /// Inserts a hash (borrowed or owned) into the accumulator.
    pub fn add_hash<E: Into<Cow<'a, Hash>>>(&mut self, hash: E) {
        self.hashes.push(hash.into());
    }

    /// Serializes an element and appends its hash to the accumulator.
    ///
    /// This is useful when adding domain-specific structures (e.g., transactions, blocks).
    pub fn add_serialized<S: Serializer>(&mut self, element: &S) {
        let serialized = element.to_bytes();
        let hashed = hash(&serialized);
        self.push_hashed(hashed);
    }

    /// Hashes a raw byte slice and appends it to the accumulator.
    pub fn add_raw_bytes(&mut self, data: &[u8]) {
        let hashed = hash(data);
        self.push_hashed(hashed);
    }

    /// Converts a fixed-size byte array into a `Hash` and adds it.
    ///
    /// Assumes the byte array is exactly `HASH_SIZE` bytes.
    pub fn add_raw_hash(&mut self, bytes: [u8; HASH_SIZE]) {
        self.push_hashed(Hash::new(bytes));
    }

    /// Internal utility to insert an owned hash.
    #[inline]
    fn push_hashed(&mut self, hashed: Hash) {
        sel


    // Build the merkle tree and return the root hash
    pub fn build(&mut self) -> Hash {
        while self.hashes.len() > 1 {
            let mut new_hashes = Vec::new();
            for i in (0..self.hashes.len()).step_by(2) {
                let left = &self.hashes[i];
                let right = if i + 1 < self.hashes.len() {
                    self.hashes[i + 1].as_ref()
                } else {
                    self.hashes[i].as_ref()
                };
                let hash = hash(&[left.as_bytes().as_ref(), right.as_bytes().as_ref()].concat());
                new_hashes.push(Cow::Owned(hash));
            }
            self.hashes = new_hashes;
        }
        debug_assert!(self.hashes.len() == 1);
        self.hashes.remove(0).into_owned()
    }

    // Verify the merkle tree with a given root hash
    pub fn verify(&mut self, root: &Hash) -> bool {
        self.build() == *root
    }
}
