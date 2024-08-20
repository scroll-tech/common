use integration::test_util::{load_batch, load_chunk, load_chunk_for_test, ASSETS_DIR, PARAMS_DIR};
use prover::{
    eth_types::H256,
    proof::dump_as_json,
    utils::{chunk_trace_to_witness_block, init_env_and_log, read_env_var},
    zkevm, BatchData, BatchHash, BatchHeader, BatchProvingTask, ChunkInfo, ChunkProvingTask,
    MAX_AGG_SNARKS,
};
use std::{env, fs, path::Path};

fn load_test_batch() -> anyhow::Result<Vec<String>> {
    let batch_dir = read_env_var("TRACE_PATH", "./tests/extra_traces/batch_25".to_string());
    load_batch(&batch_dir)
}

#[test]
fn test_batch_pi_consistency() {
    let output_dir = init_env_and_log("batch_pi");
    log::info!("Initialized ENV and created output-dir {output_dir}");
    let trace_paths = load_test_batch().unwrap();
    log_batch_pi(&trace_paths);
}

#[cfg(feature = "prove_verify")]
#[test]
fn test_e2e_prove_verify() {
    use integration::prove::{new_batch_prover, prove_and_verify_batch, prove_and_verify_bundle};

    let output_dir = init_env_and_log("e2e_tests");
    log::info!("Initialized ENV and created output-dir {output_dir}");

    let chunks1 = load_batch("./tests/extra_traces/batch1").unwrap();
    let chunks2 = load_batch("./tests/extra_traces/batch2").unwrap();

    let mut batch_prover_pending = None;
    let mut opt_batch_header = None;
    let mut batch_proofs = Vec::new();

    for (i, chunk) in [chunks1, chunks2].into_iter().enumerate() {
        let (batch, batch_header) = gen_batch_proving_task(&output_dir, &chunk, opt_batch_header);
        dump_as_json(
            &output_dir,
            format!("batch_prove_{}", i + 1).as_str(),
            &batch,
        )
        .unwrap();
        if i == 0 {
            dump_chunk_protocol(&batch, &output_dir);
            batch_prover_pending.replace(new_batch_prover(&output_dir));
        }
        let batch_prover = batch_prover_pending.as_mut().unwrap();

        let batch_proof = prove_and_verify_batch(&output_dir, batch_prover, batch);
        /*
        let proof_path = Path::new(&output_dir).join("full_proof_batch_agg.json");
        let proof_path_to =
            Path::new(&output_dir).join(format!("full_proof_batch_agg_{}.json", i + 1).as_str());
        fs::rename(proof_path, proof_path_to).unwrap();
        */
        log::info!(
            "batch proof {}, prev hash {:x?}, current {:x?}",
            i,
            batch_header.parent_batch_hash,
            batch_proof.batch_hash,
        );
        opt_batch_header.replace(batch_header);
        batch_proofs.push(batch_proof);
    }

    let batch_prover = batch_prover_pending.as_mut().unwrap();
    let bundle = prover::BundleProvingTask { batch_proofs };
    prove_and_verify_bundle(&output_dir, batch_prover, bundle);
}

#[cfg(feature = "prove_verify")]
#[test]
fn test_batch_bundle_verify() -> anyhow::Result<()> {
    use integration::{
        prove::{new_batch_prover, prove_and_verify_batch, prove_and_verify_bundle},
        test_util::read_dir,
    };
    use prover::{io::from_json_file, BundleProvingTask};

    let output_dir = init_env_and_log("batch_bundle_tests");

    let batch_tasks_paths = read_dir("./tests/test_data/batch_tasks")?;
    let batch_tasks = batch_tasks_paths
        .iter()
        .map(|path| from_json_file::<BatchProvingTask>(&path.as_path().to_string_lossy()))
        .collect::<anyhow::Result<Vec<_>>>()?;

    log::info!("num batch tasks = {}", batch_tasks.len());

    let mut prover = new_batch_prover(&output_dir);
    let batch_proofs = batch_tasks
        .into_iter()
        .map(|batch_task| prove_and_verify_batch(&output_dir, &mut prover, batch_task))
        .collect::<Vec<_>>();

    assert_eq!(batch_proofs.len(), 10, "expecting 10 batches");

    log::info!("bundle 1 batch");
    prove_and_verify_bundle(
        &output_dir,
        &mut prover,
        BundleProvingTask {
            batch_proofs: batch_proofs[0..1].to_vec(),
        },
    );
    log::info!("bundle 1 batches OK");

    log::info!("bundle 2 batches");
    prove_and_verify_bundle(
        &output_dir,
        &mut prover,
        BundleProvingTask {
            batch_proofs: batch_proofs[1..3].to_vec(),
        },
    );
    log::info!("bundle 2 batches OK");

    log::info!("bundle 3 batches");
    prove_and_verify_bundle(
        &output_dir,
        &mut prover,
        BundleProvingTask {
            batch_proofs: batch_proofs[3..6].to_vec(),
        },
    );
    log::info!("bundle 3 batches OK");

    log::info!("bundle 4 batches");
    prove_and_verify_bundle(
        &output_dir,
        &mut prover,
        BundleProvingTask {
            batch_proofs: batch_proofs[6..10].to_vec(),
        },
    );
    log::info!("bundle 4 batches OK");

    Ok(())
}

// `chunks` are unpadded
// TODO: move it elsewhere?
// Similar codes with aggregator/src/tests/aggregation.rs
// Refactor?
fn get_blob_from_chunks(chunks: &[ChunkInfo]) -> Vec<u8> {
    let num_chunks = chunks.len();

    let padded_chunk =
        ChunkInfo::mock_padded_chunk_info_for_testing(chunks.last().as_ref().unwrap());
    let chunks_with_padding = [
        chunks.to_vec(),
        vec![padded_chunk; MAX_AGG_SNARKS - num_chunks],
    ]
    .concat();
    let batch_data = BatchData::<{ MAX_AGG_SNARKS }>::new(chunks.len(), &chunks_with_padding);
    let batch_bytes = batch_data.get_batch_data_bytes();
    let blob_bytes = prover::aggregator::eip4844::get_blob_bytes(&batch_bytes);
    log::info!("blob_bytes len {}", blob_bytes.len());
    blob_bytes
}

fn gen_batch_proving_task(
    output_dir: &str,
    chunk_dirs: &[String],
    opt_batch_header: Option<BatchHeader<MAX_AGG_SNARKS>>,
) -> (BatchProvingTask, BatchHeader<MAX_AGG_SNARKS>) {
    let chunks: Vec<_> = chunk_dirs
        .iter()
        .map(|chunk_dir| load_chunk(chunk_dir).1)
        .collect();
    let l1_message_popped = chunks
        .iter()
        .flatten()
        .map(|chunk| chunk.num_l1_txs())
        .sum();
    let last_block_timestamp = chunks.last().map_or(0, |block_traces| {
        block_traces
            .last()
            .map_or(0, |block_trace| block_trace.header.timestamp.as_u64())
    });

    let mut zkevm_prover = zkevm::Prover::from_dirs(PARAMS_DIR, ASSETS_DIR);
    log::info!("Constructed zkevm prover");
    let chunk_proofs: Vec<_> = chunks
        .into_iter()
        .enumerate()
        .map(|(_, block_traces)| {
            zkevm_prover
                .gen_chunk_proof(
                    ChunkProvingTask::from(block_traces),
                    None,
                    None,
                    Some(output_dir),
                )
                .unwrap()
        })
        .collect();

    log::info!("Generated chunk proofs");

    // dummy parent batch hash
    let dummy_parent_batch_hash = H256([
        0xab, 0xac, 0xad, 0xae, 0xaf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0,
    ]);
    let chunks = chunk_proofs
        .clone()
        .into_iter()
        .map(|cp| cp.chunk_info)
        .collect::<Vec<_>>();

    let blob_bytes = get_blob_from_chunks(&chunks);

    let batch_header = BatchHeader::construct_from_chunks(
        opt_batch_header.map_or(4, |header| header.version),
        opt_batch_header.map_or(123, |header| header.batch_index + 1),
        l1_message_popped,
        opt_batch_header.map_or(l1_message_popped, |header| {
            header.total_l1_message_popped + l1_message_popped
        }),
        opt_batch_header.map_or(dummy_parent_batch_hash, |header| header.batch_hash()),
        last_block_timestamp,
        &chunks,
        &blob_bytes,
    );

    (
        BatchProvingTask {
            chunk_proofs,
            batch_header,
            blob_bytes,
        },
        batch_header,
    )
}

fn log_batch_pi(trace_paths: &[String]) {
    let max_num_snarks = prover::MAX_AGG_SNARKS;
    let chunk_traces: Vec<_> = trace_paths
        .iter()
        .map(|trace_path| {
            env::set_var("TRACE_PATH", trace_path);
            load_chunk_for_test().1
        })
        .collect();
    let l1_message_popped = chunk_traces
        .iter()
        .flatten()
        .map(|chunk| chunk.num_l1_txs())
        .sum();
    let last_block_timestamp = chunk_traces.last().map_or(0, |block_traces| {
        block_traces
            .last()
            .map_or(0, |block_trace| block_trace.header.timestamp.as_u64())
    });

    let mut chunk_hashes: Vec<ChunkInfo> = chunk_traces
        .into_iter()
        .enumerate()
        .map(|(_i, chunk_trace)| {
            let witness_block = chunk_trace_to_witness_block(chunk_trace.clone()).unwrap();
            ChunkInfo::from_witness_block(&witness_block, false)
        })
        .collect();
    let blob_bytes = get_blob_from_chunks(&chunk_hashes);
    let real_chunk_count = chunk_hashes.len();
    if real_chunk_count < max_num_snarks {
        let mut padding_chunk_hash = chunk_hashes.last().unwrap().clone();
        padding_chunk_hash.is_padding = true;

        // Extend to MAX_AGG_SNARKS for both chunk hashes and layer-2 snarks.
        chunk_hashes
            .extend(std::iter::repeat(padding_chunk_hash).take(max_num_snarks - real_chunk_count));
    }

    // dummy parent batch hash
    let parent_batch_hash = H256([
        0xab, 0xac, 0xad, 0xae, 0xaf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0,
    ]);
    let batch_header = BatchHeader {
        version: 3,
        batch_index: 123,
        l1_message_popped,
        total_l1_message_popped: l1_message_popped,
        parent_batch_hash,
        last_block_timestamp,
        ..Default::default() // these will be populated later.
    };
    let batch_hash = BatchHash::<{ prover::MAX_AGG_SNARKS }>::construct(
        &chunk_hashes,
        batch_header,
        &blob_bytes,
    );
    let blob = batch_hash.point_evaluation_assignments();

    let challenge = blob.challenge;
    let evaluation = blob.evaluation;
    println!("blob.challenge: {challenge:x}");
    println!("blob.evaluation: {evaluation:x}");
    for (i, elem) in blob.coefficients.iter().enumerate() {
        println!("blob.coeffs[{}]: {elem:x}", i);
    }
}

fn dump_chunk_protocol(batch: &BatchProvingTask, output_dir: &str) {
    // Dump chunk-procotol to "chunk_chunk_0.protocol" for batch proving.
    batch
        .chunk_proofs
        .first()
        .unwrap()
        .dump(output_dir, "0")
        .unwrap();
}
