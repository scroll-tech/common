use std::marker::PhantomData;

use eth_types::{Field, H256};
use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner},
    plonk::{Circuit, ConstraintSystem, Error},
};

use zkevm_circuits::util::{Challenges, SubCircuitConfig};

use super::{
    batch::BatchHash,
    chunk::ChunkHash,
    config::{BatchCircuitConfig, BatchCircuitConfigArgs},
};

/// BatchCircuit struct.
///
/// Contains public inputs and witnesses that are needed to
/// generate the circuit.
#[derive(Clone, Debug, Default)]
pub struct BatchHashCircuit<F: Field> {
    pub(crate) chain_id: u8,
    pub(crate) chunks: Vec<ChunkHash>,
    pub(crate) batch: BatchHash,
    _phantom: PhantomData<F>,
}

/// Public input to a batch circuit.
/// In raw format. I.e., before converting to field elements.
pub struct BatchHashCircuitPublicInput {
    pub(crate) chain_id: u8,
    pub(crate) first_chunk_prev_state_root: H256,
    pub(crate) last_chunk_post_state_root: H256,
    pub(crate) last_chunk_withdraw_root: H256,
    pub(crate) batch_public_input_hash: H256,
}

impl<F: Field> BatchHashCircuit<F> {
    /// Sample a batch hash circuit from random (for testing)
    #[cfg(test)]
    pub(crate) fn mock_batch_hash_circuit<R: rand::RngCore>(r: &mut R, size: usize) -> Self {
        let mut chunks = (0..size)
            .map(|_| ChunkHash::mock_chunk_hash(r))
            .collect::<Vec<_>>();
        for i in 0..size - 1 {
            chunks[i + 1].prev_state_root = chunks[i].post_state_root;
        }

        Self::construct(&chunks, 0)
    }

    /// Build Batch hash circuit from a list of chunks
    pub fn construct(chunk_hashes: &[ChunkHash], chain_id: u8) -> Self {
        let batch = BatchHash::construct(chunk_hashes, chain_id);
        Self {
            chain_id,
            chunks: chunk_hashes.to_vec(),
            batch,
            _phantom: PhantomData::default(),
        }
    }

    /// The public input to the BatchHashCircuit
    pub fn public_input(&self) -> BatchHashCircuitPublicInput {
        BatchHashCircuitPublicInput {
            chain_id: self.chain_id,
            first_chunk_prev_state_root: self.chunks[0].prev_state_root,
            last_chunk_post_state_root: self.chunks.last().unwrap().post_state_root,
            last_chunk_withdraw_root: self.chunks.last().unwrap().withdraw_root,
            batch_public_input_hash: self.batch.public_input_hash,
        }
    }

    /// Extract all the hash inputs that will ever be used
    /// orders:
    /// - batch_public_input_hash
    /// - batch_data_hash_preimage
    /// - chunk[i].piHash for i in [0, k)
    pub(crate) fn extract_hash_preimages(&self) -> Vec<Vec<u8>> {
        let mut res = vec![];

        // batchPiHash =
        //  keccak(
        //      chain_id ||
        //      chunk[0].prev_state_root ||
        //      chunk[k-1].post_state_root ||
        //      chunk[k-1].withdraw_root ||
        //      batch_data_hash )
        let batch_public_input_hash_preimage = [
            &[self.chain_id],
            self.chunks[0].prev_state_root.as_bytes(),
            self.chunks.last().unwrap().post_state_root.as_bytes(),
            self.chunks.last().unwrap().withdraw_root.as_bytes(),
            self.batch.data_hash.as_bytes(),
        ]
        .concat();
        res.push(batch_public_input_hash_preimage);

        // batchDataHash = keccak(chunk[0].dataHash || ... || chunk[k-1].dataHash)
        let batch_data_hash_preimage = self
            .chunks
            .iter()
            .flat_map(|x| x.data_hash.as_bytes().iter())
            .cloned()
            .collect();
        res.push(batch_data_hash_preimage);

        // compute piHash for each chunk for i in [0..k)
        // chunk[i].piHash =
        // keccak(
        //        chain id ||
        //        chunk[i].prevStateRoot || chunk[i].postStateRoot || chunk[i].withdrawRoot ||
        //        chunk[i].datahash)
        for chunk in self.chunks.iter() {
            let chunk_pi_hash_preimage = [
                &[self.chain_id],
                chunk.prev_state_root.as_bytes(),
                chunk.post_state_root.as_bytes(),
                chunk.withdraw_root.as_bytes(),
                chunk.data_hash.as_bytes(),
            ]
            .concat();
            res.push(chunk_pi_hash_preimage)
        }

        res
    }
}

impl<F: Field> Circuit<F> for BatchHashCircuit<F> {
    type FloorPlanner = SimpleFloorPlanner;

    type Config = (BatchCircuitConfig<F>, Challenges);

    fn without_witnesses(&self) -> Self {
        Self::default()
    }
    fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
        let challenges = Challenges::construct(meta);
        let challenges_exprs = challenges.exprs(meta);
        let args = BatchCircuitConfigArgs {
            challenges: challenges_exprs,
        };
        let config = BatchCircuitConfig::new(meta, args);
        (config, challenges)
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<F>,
    ) -> Result<(), Error> {
        let (config, challenge) = config;
        let challenges = challenge.values(&layouter);

        // extract all the hashes and load them to the hash table
        let preimages = self.extract_hash_preimages();

        config
            .keccak_circuit_config
            .load_aux_tables(&mut layouter)?;
        config.assign(&mut layouter, challenges, &preimages)?;

        Ok(())
    }
}
