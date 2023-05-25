//! This module implements circuits that aggregates public inputs of many chunks into a single
//! one.
//!
//! # Spec
//!
//! A chunk is a list of continuous blocks. It consists of 4 hashes:
//! - state root before this chunk
//! - state root after this chunk
//! - the withdraw root of this chunk
//! - the data hash of this chunk
//! Those 4 hashes are obtained from the caller.
//!
//! A chunk's public input hash is then derived from the above 4 attributes via
//! - chunk_pi_hash   := keccak(chain_id    ||
//!                         prev_state_root ||
//!                         post_state_root ||
//!                         withdraw_root   ||
//!                         chunk_data_hash)
//!
//! A batch is a list of continuous chunks. It consists of 2 hashes
//! - batch_data_hash := keccak(chunk_0.data_hash      ||
//!                         ...                        ||
//!                         chunk_k-1.data_hash)
//!
//! - batch_pi_hash   := keccak(chain_id               ||  
//!                         chunk_0.prev_state_root    ||
//!                         chunk_k-1.post_state_root  ||
//!                         chunk_k-1.withdraw_root    ||
//!                         batch_data_hash)
//!
//! Note that chain_id is used for all public input hashes. But not for any data hashes.
//!
//! # Circuit
//!
//! A BatchHashCircuit asserts that the batch is well-formed.
//!
//! ## Public Input
//! The public inputs of the circuit (129 Field elements) is constructed as
//! - first_chunk_prev_state_root: 32 Field elements
//! - last_chunk_post_state_root: 32 Field elements
//! - last_chunk_withdraw_root: 32 Field elements
//! - batch_public_input_hash: 32 Field elements
//! - chain_id: 1 Field element
//!
//! ## Constraints
//! The circuit attests the following statements:
//!
//! 1. all hashes are computed correctly
//! 2. the relations between hash preimages and digests are satisfied
//!     - batch_data_hash is part of the input to compute batch_pi_hash
//!     - batch_pi_hash used same roots as chunk_pi_hash
//!     - same data_hash is used to compute batch_data_hash and chunk_pi_hash for all chunks
//!     - chunks are continuous: they are linked via the state roots
//!     - all hashes uses a same chain_id
//! 3. the hash data matches the circuit's public input (129 field elements) above
//!
//! # Example
//!
//! See tests::test_pi_aggregation_circuit

// This module implements `Chunk` related data types.
// A chunk is a list of blocks.
mod chunk;
// This module implements `Batch` related data types.
// A batch is a list of chunk.
mod batch;
// Circuit implementation of `BatchHash`.
mod circuit;
// SubCircuit implementation of `BatchHash`.
mod sub_circuit;
// Circuit and SubCircuit configurations
mod config;
/// utilities
mod util;

pub use batch::BatchHash;
pub use chunk::ChunkHash;
pub use circuit::{BatchHashCircuit, BatchHashCircuitPublicInput};
pub use config::{BatchCircuitConfig, BatchCircuitConfigArgs};

// TODO(ZZ): update to the right degree
pub(crate) const LOG_DEGREE: u32 = 19;

// Each round requires (NUM_ROUNDS+1) * DEFAULT_KECCAK_ROWS = 300 rows.
// This library is hard coded for this parameter.
// Modifying the following parameters may result into bugs.
// Adopted from keccak circuit
pub(crate) const DEFAULT_KECCAK_ROWS: usize = 12;
// Adopted from keccak circuit
pub(crate) const NUM_ROUNDS: usize = 24;

#[cfg(test)]
mod tests;
