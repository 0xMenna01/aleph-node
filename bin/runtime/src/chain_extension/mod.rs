use codec::{Decode, Encode};
use environment::Environment;
use executor::Executor;
use frame_support::{log::error, weights::Weight};
use pallet_baby_liminal::{Config, Error, ProvingSystem, VerificationKeyIdentifier, WeightInfo};
use pallet_contracts::chain_extension::{
    ChainExtension, Environment as SubstrateEnvironment, Ext, InitState, RetVal, SysConfig,
};
use primitives::host_functions::poseidon;
use sp_core::crypto::UncheckedFrom;
use sp_runtime::DispatchError;
use sp_std::{mem::size_of, vec::Vec};
use Error::*;

use crate::{MaximumVerificationKeyLength, Runtime};
mod environment;
mod executor;
#[cfg(test)]
mod tests;

pub const BABY_LIMINAL_STORE_KEY_FUNC_ID: u32 = 41;
pub const BABY_LIMINAL_VERIFY_FUNC_ID: u32 = 42;
pub const POSEIDON_ONE_TO_ONE_FUNC_ID: u32 = 43;
pub const POSEIDON_TWO_TO_ONE_FUNC_ID: u32 = 44;
pub const POSEIDON_FOUR_TO_ONE_FUNC_ID: u32 = 45;

// Return codes for `BABY_LIMINAL_STORE_KEY_FUNC_ID`.
pub const BABY_LIMINAL_STORE_KEY_OK: u32 = 10_000;
pub const BABY_LIMINAL_STORE_KEY_TOO_LONG_KEY: u32 = 10_001;
pub const BABY_LIMINAL_STORE_KEY_IDENTIFIER_IN_USE: u32 = 10_002;
pub const BABY_LIMINAL_STORE_KEY_ERROR_UNKNOWN: u32 = 10_003;

// Return codes for `BABY_LIMINAL_VERIFY_FUNC_ID`.
pub const BABY_LIMINAL_VERIFY_OK: u32 = 11_000;
pub const BABY_LIMINAL_VERIFY_DESERIALIZING_PROOF_FAIL: u32 = 11_001;
pub const BABY_LIMINAL_VERIFY_DESERIALIZING_INPUT_FAIL: u32 = 11_002;
pub const BABY_LIMINAL_VERIFY_UNKNOWN_IDENTIFIER: u32 = 11_003;
pub const BABY_LIMINAL_VERIFY_DESERIALIZING_KEY_FAIL: u32 = 11_004;
pub const BABY_LIMINAL_VERIFY_VERIFICATION_FAIL: u32 = 11_005;
pub const BABY_LIMINAL_VERIFY_INCORRECT_PROOF: u32 = 11_006;
pub const BABY_LIMINAL_VERIFY_ERROR_UNKNOWN: u32 = 11_007;

// Return codes for `POSEIDON_*_FUNC_ID`.
pub const POSEIDON_X_TO_ONE_OK: u32 = 12_000;

#[derive(Default)]
pub struct BabyLiminalChainExtension;

impl ChainExtension<Runtime> for BabyLiminalChainExtension {
    fn call<E: Ext>(
        &mut self,
        env: SubstrateEnvironment<E, InitState>,
    ) -> Result<RetVal, DispatchError>
    where
        <E::T as SysConfig>::AccountId: UncheckedFrom<<E::T as SysConfig>::Hash> + AsRef<[u8]>,
    {
        let func_id = env.func_id() as u32;

        match func_id {
            BABY_LIMINAL_STORE_KEY_FUNC_ID => {
                Self::baby_liminal_store_key::<_, Runtime>(env.buf_in_buf_out())
            }
            BABY_LIMINAL_VERIFY_FUNC_ID => {
                Self::baby_liminal_verify::<_, Runtime>(env.buf_in_buf_out())
            }
            POSEIDON_ONE_TO_ONE_FUNC_ID => {
                Self::poseidon_one_to_one::<_, Runtime>(env.buf_in_buf_out())
            }
            POSEIDON_TWO_TO_ONE_FUNC_ID => {
                Self::poseidon_two_to_one::<_, Runtime>(env.buf_in_buf_out())
            }
            POSEIDON_FOUR_TO_ONE_FUNC_ID => {
                Self::poseidon_four_to_one::<_, Runtime>(env.buf_in_buf_out())
            }
            _ => {
                error!("Called an unregistered `func_id`: {}", func_id);
                Err(DispatchError::Other("Unimplemented func_id"))
            }
        }
    }
}

pub type ByteCount = u32;

/// Struct to be decoded from a byte slice passed from the contract.
///
/// Notice, that contract can pass these arguments one by one, not necessarily as such struct. Only
/// the order of values is important.
///
/// It cannot be `MaxEncodedLen` due to `Vec<_>` and thus `Environment::read_as` cannot be used.
#[derive(Decode, Encode)]
struct StoreKeyArgs {
    pub identifier: VerificationKeyIdentifier,
    pub key: Vec<u8>,
}

/// Struct to be decoded from a byte slice passed from the contract.
///
/// Notice, that contract can pass these arguments one by one, not necessarily as such struct. Only
/// the order of values is important.
///
/// It cannot be `MaxEncodedLen` due to `Vec<_>` and thus `Environment::read_as` cannot be used.
#[derive(Decode, Encode)]
struct VerifyArgs {
    pub identifier: VerificationKeyIdentifier,
    pub proof: Vec<u8>,
    pub input: Vec<u8>,
    pub system: ProvingSystem,
}

/// Provides a weight of `store_key` dispatchable.
fn weight_of_store_key(key_length: ByteCount) -> Weight {
    <<Runtime as Config>::WeightInfo as WeightInfo>::store_key(key_length)
}

/// Provides a weight of `verify` dispatchable depending on the `ProvingSystem`. In case no system
/// is passed, we return maximal amongst all the systems.
fn weight_of_verify(system: Option<ProvingSystem>) -> Weight {
    match system {
        Some(ProvingSystem::Groth16) => {
            <<Runtime as Config>::WeightInfo as WeightInfo>::verify_groth16()
        }
        Some(ProvingSystem::Gm17) => <<Runtime as Config>::WeightInfo as WeightInfo>::verify_gm17(),
        Some(ProvingSystem::Marlin) => {
            <<Runtime as Config>::WeightInfo as WeightInfo>::verify_marlin()
        }
        None => weight_of_verify(Some(ProvingSystem::Groth16))
            .max(weight_of_verify(Some(ProvingSystem::Gm17)))
            .max(weight_of_verify(Some(ProvingSystem::Marlin))),
    }
}

type SingleHashInput = primitives::host_functions::Input;
impl BabyLiminalChainExtension {
    fn baby_liminal_store_key<Env: Environment, Exc: Executor>(
        mut env: Env,
    ) -> Result<RetVal, DispatchError> {
        // Check if it makes sense to read and decode data. This is only an upperbound for the key
        // length, because this bytes suffix contains (possibly compressed) info about actual key
        // length (needed for decoding).
        let approx_key_length = env
            .in_len()
            .saturating_sub(size_of::<VerificationKeyIdentifier>() as ByteCount);
        if approx_key_length > MaximumVerificationKeyLength::get() {
            return Ok(RetVal::Converging(BABY_LIMINAL_STORE_KEY_TOO_LONG_KEY));
        }

        // We charge now - even if decoding fails and we shouldn't touch storage, we have to incur
        // fee for reading memory.
        let pre_charged = env.charge_weight(weight_of_store_key(approx_key_length))?;

        // Parsing will have to be done here. This is due to the fact that methods
        // `Environment<_,_,_,S: BufIn>::read*` don't move starting pointer and thus we can make
        // only a single read. Since `key` is just an ('unbounded') `Vec<u8>` we can only use
        // `env.read()` method and decode arguments by hand here.
        //
        // It is safe to read `env.in_len()` bytes since we already checked that it's not too much.
        let bytes = env.read(env.in_len())?;

        let args = StoreKeyArgs::decode(&mut &*bytes)
            .map_err(|_| DispatchError::Other("Failed to decode arguments"))?;

        // Now we know the exact key length.
        env.adjust_weight(
            pre_charged,
            weight_of_store_key(args.key.len() as ByteCount),
        );

        let return_status = match Exc::store_key(args.identifier, args.key) {
            Ok(_) => BABY_LIMINAL_STORE_KEY_OK,
            // In case `DispatchResultWithPostInfo` was returned (or some simpler equivalent for
            // `bare_store_key`), we could have adjusted weight. However, for the storing key action
            // it doesn't make much sense.
            Err(VerificationKeyTooLong) => BABY_LIMINAL_STORE_KEY_TOO_LONG_KEY,
            Err(IdentifierAlreadyInUse) => BABY_LIMINAL_STORE_KEY_IDENTIFIER_IN_USE,
            _ => BABY_LIMINAL_STORE_KEY_ERROR_UNKNOWN,
        };
        Ok(RetVal::Converging(return_status))
    }

    fn baby_liminal_verify<Env: Environment, Exc: Executor>(
        mut env: Env,
    ) -> Result<RetVal, DispatchError> {
        // We charge optimistically, i.e. assuming that decoding succeeds and the verification
        // key is present. However, since we don't know the system yet, we have to charge maximal
        // possible fee. We will adjust it as soon as possible.
        let pre_charge = env.charge_weight(weight_of_verify(None))?;

        // Parsing is done here for similar reasons as in `Self::baby_liminal_store_key`.
        let bytes = env.read(env.in_len())?;

        let args: VerifyArgs = VerifyArgs::decode(&mut &*bytes)
            .map_err(|_| DispatchError::Other("Failed to decode arguments"))?;

        let result = Exc::verify(args.identifier, args.proof, args.input, args.system);

        // Adjust weight
        match &result {
            // Positive case: we can adjust weight based on the system used.
            Ok(_) => env.adjust_weight(pre_charge, weight_of_verify(Some(args.system))),
            // Negative case: Now we inspect how we should adjust weighting. In case pallet provides
            // us with post-dispatch weight, we will use it. Otherwise, we weight the call in the
            // same way as in the positive case.
            Err((_, Some(actual_weight))) => env.adjust_weight(pre_charge, *actual_weight),
            Err((_, None)) => env.adjust_weight(pre_charge, weight_of_verify(Some(args.system))),
        };

        let return_status = match result {
            Ok(_) => BABY_LIMINAL_VERIFY_OK,
            Err((error, _)) => match error {
                DeserializingProofFailed => BABY_LIMINAL_VERIFY_DESERIALIZING_PROOF_FAIL,
                DeserializingPublicInputFailed => BABY_LIMINAL_VERIFY_DESERIALIZING_INPUT_FAIL,
                UnknownVerificationKeyIdentifier => BABY_LIMINAL_VERIFY_UNKNOWN_IDENTIFIER,
                DeserializingVerificationKeyFailed => BABY_LIMINAL_VERIFY_DESERIALIZING_KEY_FAIL,
                VerificationFailed(_) => BABY_LIMINAL_VERIFY_VERIFICATION_FAIL,
                IncorrectProof => BABY_LIMINAL_VERIFY_INCORRECT_PROOF,
                _ => BABY_LIMINAL_VERIFY_ERROR_UNKNOWN,
            },
        };
        Ok(RetVal::Converging(return_status))
    }

    fn poseidon_one_to_one<Env: Environment, Exc: Executor>(
        mut env: Env,
    ) -> Result<RetVal, DispatchError> {
        let input: [SingleHashInput; 1] = env.read_as()?;

        env.charge_weight(
            <<Runtime as Config>::WeightInfo as WeightInfo>::poseidon_one_to_one_host(),
        )?;
        let hash = poseidon::one_to_one_hash(input[0]);

        // hash is of type (u64, u64, u64, u64) but this type has the same encoding
        // as [u64, u64, u64, u64] so we're fine
        env.write(&hash.encode())?;

        Ok(RetVal::Converging(POSEIDON_X_TO_ONE_OK))
    }

    fn poseidon_two_to_one<Env: Environment, Exc: Executor>(
        mut env: Env,
    ) -> Result<RetVal, DispatchError> {
        let input: [SingleHashInput; 2] = env.read_as()?;

        env.charge_weight(
            <<Runtime as Config>::WeightInfo as WeightInfo>::poseidon_two_to_one_host(),
        )?;
        let hash = poseidon::two_to_one_hash(input[0], input[1]);

        // hash is of type (u64, u64, u64, u64) but this type has the same encoding
        // as [u64, u64, u64, u64] so we're fine
        env.write(&hash.encode())?;

        Ok(RetVal::Converging(POSEIDON_X_TO_ONE_OK))
    }

    fn poseidon_four_to_one<Env: Environment, Exc: Executor>(
        mut env: Env,
    ) -> Result<RetVal, DispatchError> {
        let input: [SingleHashInput; 4] = env.read_as()?;

        env.charge_weight(
            <<Runtime as Config>::WeightInfo as WeightInfo>::poseidon_four_to_one_host(),
        )?;
        let hash = poseidon::four_to_one_hash(input[0], input[1], input[2], input[3]);

        // hash is of type (u64, u64, u64, u64) but this type has the same encoding
        // as [u64, u64, u64, u64] so we're fine
        env.write(&hash.encode())?;

        Ok(RetVal::Converging(POSEIDON_X_TO_ONE_OK))
    }
}