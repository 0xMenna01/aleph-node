#![allow(clippy::let_unit_value)]

use frame_benchmarking::{account, benchmarks, vec, Vec};
use frame_support::{traits::Get, BoundedVec};
use frame_system::RawOrigin;

use crate::{
    benchmarking::import::Artifacts, get_artifacts, Call, Config, Pallet, ProvingSystem::*,
    VerificationKeyIdentifier, VerificationKeys,
};

const SEED: u32 = 41;
const IDENTIFIER: VerificationKeyIdentifier = [0; 4];

fn caller<T: Config>() -> RawOrigin<<T as frame_system::Config>::AccountId> {
    RawOrigin::Signed(account("caller", 0, SEED))
}

fn insert_key<T: Config>(key: Vec<u8>) {
    VerificationKeys::<T>::insert(IDENTIFIER, BoundedVec::try_from(key).unwrap());
}

benchmarks! {

    store_key {
        let l in 1 .. T::MaximumVerificationKeyLength::get();
        let key = vec![0u8; l as usize];
    } : _(caller::<T>(), IDENTIFIER, key)

    overwrite_key {
        let l in 1 .. T::MaximumVerificationKeyLength::get();
        let key = vec![0u8; l as usize];
    } : _(RawOrigin::Root, IDENTIFIER, key)

    // Groth16 benchmarks

    verify_groth16_xor {
        let Artifacts { key, proof, input } = get_artifacts!(Groth16, Xor);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Groth16)

    verify_groth16_linear_equation {
        let Artifacts { key, proof, input } = get_artifacts!(Groth16, LinearEquation);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Groth16)

    verify_groth16_merkle_tree_8 {
        let Artifacts { key, proof, input } = get_artifacts!(Groth16, MerkleTree8);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Groth16)

    verify_groth16_merkle_tree_64 {
        let Artifacts { key, proof, input } = get_artifacts!(Groth16, MerkleTree64);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Groth16)

    verify_groth16_merkle_tree_1024 {
        let Artifacts { key, proof, input } = get_artifacts!(Groth16, MerkleTree1024);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Groth16)

    // GM17 benchmarks

    verify_gm17_xor {
        let Artifacts { key, proof, input } = get_artifacts!(Gm17, Xor);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Gm17)

    verify_gm17_linear_equation {
        let Artifacts { key, proof, input } = get_artifacts!(Gm17, LinearEquation);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Gm17)

    verify_gm17_merkle_tree_8 {
        let Artifacts { key, proof, input } = get_artifacts!(Gm17, MerkleTree8);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Gm17)

    verify_gm17_merkle_tree_64 {
        let Artifacts { key, proof, input } = get_artifacts!(Gm17, MerkleTree64);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Gm17)

    verify_gm17_merkle_tree_1024 {
        let Artifacts { key, proof, input } = get_artifacts!(Gm17, MerkleTree1024);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Gm17)

    // Marlin benchmarks

    verify_marlin_xor {
        let Artifacts { key, proof, input } = get_artifacts!(Marlin, Xor);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Marlin)

    verify_marlin_linear_equation {
        let Artifacts { key, proof, input } = get_artifacts!(Marlin, LinearEquation);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Marlin)

    verify_marlin_merkle_tree_8 {
        let Artifacts { key, proof, input } = get_artifacts!(Marlin, MerkleTree8);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Marlin)

    verify_marlin_merkle_tree_64 {
        let Artifacts { key, proof, input } = get_artifacts!(Marlin, MerkleTree64);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Marlin)

    verify_marlin_merkle_tree_1024 {
        let Artifacts { key, proof, input } = get_artifacts!(Marlin, MerkleTree1024);
        let _ = insert_key::<T>(key);
    } : verify(caller::<T>(), IDENTIFIER, proof, input, Marlin)

    // Partial `verify` execution

    verify_data_too_long {
        // Excess. Unfortunately, anything like
        // `let e in (T::MaximumDataLength::get() + 1) .. (T::MaximumDataLength::get() * 1_000)`
        // doesn't compile.
        let e in 1 .. T::MaximumDataLength::get() * 1_000;
        let proof = vec![255u8; (T::MaximumDataLength::get() + e) as usize];
        let Artifacts { key, proof: _proof, input } = get_artifacts!(Groth16, MerkleTree1024);
    } : {
        assert!(
            Pallet::<T>::verify(caller::<T>().into(), IDENTIFIER, proof, input, Groth16).is_err()
        )
    }

    // It shouldn't matter whether deserializing of proof fails, but for input it succeeds, or the
    // other way round. The only thing that is important is that we don't read storage nor run
    // verification procedure.
    verify_data_deserializing_fails {
        let l in 1 .. T::MaximumDataLength::get();
        let proof = vec![255u8; l as usize];
        // System shouldn't have any serious impact on deserializing - the data is just some
        // elements from the field.
        let Artifacts { key, proof: _proof, input } = get_artifacts!(Groth16, MerkleTree1024);
    } : {
        assert!(
            Pallet::<T>::verify(caller::<T>().into(), IDENTIFIER, proof, input, Groth16).is_err()
        )
    }

    verify_key_deserializing_fails {
        let l in 1 .. T::MaximumVerificationKeyLength::get();
        let _ = insert_key::<T>(vec![255u8; l as usize]);

        // System shouldn't have any serious impact on deserializing - the data is just some
        // elements from the field.
        let Artifacts { key, proof, input } = get_artifacts!(Groth16, MerkleTree1024);
    } : {
        assert!(
            Pallet::<T>::verify(caller::<T>().into(), IDENTIFIER, proof, input, Groth16).is_err()
        )
    }

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::TestRuntime);
}