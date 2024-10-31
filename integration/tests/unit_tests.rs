// Fast tests which can be finished within minutes

use integration::{
    capacity_checker::{prepare_circuit_capacity_checker, run_circuit_capacity_checker, CCCMode},
    test_util::load_chunk_for_test,
};
use prover::{
    io::read_all,
    utils::{init_env_and_log, short_git_version},
    zkevm::circuit::{block_traces_to_witness_block, TargetCircuit},
    EvmProof,
};

#[test]
fn test_short_git_version() {
    init_env_and_log("integration");

    let git_version = short_git_version();
    log::info!("short_git_version = {git_version}");

    assert_eq!(git_version.len(), 7);
}

#[ignore]
#[test]
fn test_evm_verifier() {
    init_env_and_log("test_evm_verifer");
    log::info!("cwd {:?}", std::env::current_dir());
    let version = "release-v0.11.1";
    let yul = read_all(&format!("../{version}/evm_verifier.yul"));
    //log::info!("yul len {}", yul.len());
    let pi = read_all(&format!("../{version}/pi_data.data"));
    let mut proof = read_all(&format!("../{version}/proof.data"));
    proof.splice(384..384, pi);
    log::info!("calldata len {}", proof.len());

    for version in [
        "0.8.19", "0.8.20", "0.8.21", "0.8.22", "0.8.23", "0.8.24", "0.8.25", "0.8.26",
    ] {
        use snark_verifier::loader::evm::compile_yul;
        use std::process::Command;
        Command::new("svm")
            .arg("use")
            .arg(version)
            .output()
            .expect("failed to execute process");
        log::info!("svm use {}", version);
        let bytecode = compile_yul(&String::from_utf8(yul.clone()).unwrap());
        log::info!("bytecode len {}", bytecode.len());
        match integration::evm::deploy_and_call(bytecode, proof.clone()) {
            Ok(gas) => log::info!("gas cost {gas}"),
            Err(e) => {
                panic!("test failed {e:#?}");
            }
        }
    }

    log::info!("check released bin");
    let bytecode = read_all(&format!("../{version}/evm_verifier.bin"));
    log::info!("bytecode len {}", bytecode.len());
    match integration::evm::deploy_and_call(bytecode, proof.clone()) {
        Ok(gas) => log::info!("gas cost {gas}"),
        Err(e) => {
            panic!("test failed {e:#?}");
        }
    }
}

#[ignore]
#[test]
fn test_evm_verifier_for_dumped_proof() {
    use prover::io::from_json_file;

    init_env_and_log("test_evm_verifer");
    log::info!("cwd {:?}", std::env::current_dir());

    let search_pattern = "outputs/e2e_tests_*/full_proof_evm_layer6_*.json";

    let mut paths = glob::glob(search_pattern).expect("Failed to read glob pattern");

    let mut path = paths.last().unwrap().unwrap();
    log::info!("proof path {}", path.display());
    let evm_proof: EvmProof = from_json_file(&path).unwrap();

    log::trace!("pi dump {:#?}", evm_proof.proof.instances());
    let proof: prover::BundleProof = evm_proof.proof.into();

    let proof = proof.calldata();
    log::info!("calldata len {}", proof.len());

    log::info!("check released bin");
    path.pop();
    path.push("evm_verifier.bin");
    let bytecode = read_all(path);
    log::info!("bytecode len {}", bytecode.len());
    match integration::evm::deploy_and_call(bytecode, proof.clone()) {
        Ok(gas) => log::info!("gas cost {gas}"),
        Err(e) => {
            panic!("test failed {e:#?}");
        }
    }
}

#[test]
fn test_capacity_checker() {
    init_env_and_log("integration");
    prepare_circuit_capacity_checker();

    let block_traces = load_chunk_for_test().1;

    let batch_id = 0;
    let chunk_id = 0;
    let ccc_modes = [
        CCCMode::Optimal,
        //CCCMode::Siger,
        //CCCMode::FollowerLight,
        CCCMode::FollowerFull,
    ];
    run_circuit_capacity_checker(batch_id, chunk_id, &block_traces, &ccc_modes);
}

#[test]
fn estimate_circuit_rows() {
    init_env_and_log("integration");
    prepare_circuit_capacity_checker();

    let (_, block_trace) = load_chunk_for_test();
    let witness_block = block_traces_to_witness_block(block_trace).unwrap();
    log::info!("estimating used rows");
    let row_usage = <prover::zkevm::circuit::SuperCircuit as TargetCircuit>::Inner::min_num_rows_block_subcircuits(&witness_block);
    let r = row_usage
        .iter()
        .max_by_key(|x| x.row_num_real)
        .unwrap()
        .clone();
    log::info!("final rows: {} {}", r.row_num_real, r.name);
}
