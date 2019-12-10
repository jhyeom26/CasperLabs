use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::mem::MaybeUninit;

use super::{
    alloc_bytes,
    error::{result_from, Error},
    str_ref_to_ptr, to_ptr, ContractRef, TURef,
};
use crate::{
    args_parser::ArgsParser,
    bytesrepr::{self, deserialize, FromBytes, ToBytes},
    execution::{Phase, PHASE_SIZE},
    ext_ffi,
    key::Key,
    unwrap_or_revert::UnwrapOrRevert,
    uref::URef,
    value::{
        account::{BlockTime, PublicKey, BLOCKTIME_SER_SIZE},
        Contract, Value,
    },
};

/// Return `t` to the host, terminating the currently running module.
/// Note this function is only relevant to contracts stored on chain which
/// return a value to their caller. The return value of a directly deployed
/// contract is never looked at.
#[allow(clippy::ptr_arg)]
pub fn ret<T: ToBytes>(t: T, extra_urefs: Vec<URef>) -> ! {
    let (ptr, size, _bytes) = to_ptr(&t);
    let (urefs_ptr, urefs_size, _bytes2) = to_ptr(&extra_urefs);
    unsafe {
        ext_ffi::ret(ptr, size, urefs_ptr, urefs_size);
    }
}

/// Stops execution of a contract and reverts execution effects with a given reason.
pub fn revert<T: Into<Error>>(error: T) -> ! {
    unsafe {
        ext_ffi::revert(error.into().into());
    }
}

/// Call the given contract, passing the given (serialized) arguments to
/// the host in order to have them available to the called contract during its
/// execution. The value returned from the contract call (see `ret` above) is
/// returned from this function.
#[allow(clippy::ptr_arg)]
pub fn call_contract<A: ArgsParser, T: FromBytes>(
    c_ptr: ContractRef,
    args: &A,
    extra_urefs: &Vec<Key>,
) -> T {
    let contract_key: Key = c_ptr.into();
    let (key_ptr, key_size, _bytes1) = to_ptr(&contract_key);
    let (args_ptr, args_size, _bytes2) = ArgsParser::parse(args)
        .map(|args| to_ptr(&args))
        .unwrap_or_revert();
    let (urefs_ptr, urefs_size, _bytes3) = to_ptr(extra_urefs);

    let bytes_written = {
        let mut bytes_written = MaybeUninit::uninit();
        let ret = unsafe {
            ext_ffi::call_contract(
                key_ptr,
                key_size,
                args_ptr,
                args_size,
                urefs_ptr,
                urefs_size,
                bytes_written.as_mut_ptr(),
            )
        };
        result_from(ret).unwrap_or_revert();
        unsafe { bytes_written.assume_init() }
    };
    let result = read_host_buffer_count(bytes_written).unwrap_or_revert();
    deserialize(&result).unwrap_or_revert()
}

/// Takes the name of a function to store and a contract URef, and overwrites the value under
/// that URef with a new Contract instance containing the original contract's named_keys, the
/// current protocol version, and the newly created bytes of the stored function.
pub fn upgrade_contract_at_uref(name: &str, uref: TURef<Contract>) {
    let (name_ptr, name_size, _bytes) = str_ref_to_ptr(name);
    let key: Key = uref.into();
    let (key_ptr, key_size, _bytes) = to_ptr(&key);
    let result_value =
        unsafe { ext_ffi::upgrade_contract_at_uref(name_ptr, name_size, key_ptr, key_size) };
    match result_from(result_value) {
        Ok(()) => (),
        Err(error) => revert(error),
    }
}

fn load_arg(index: u32) -> Option<usize> {
    let arg_size = unsafe { ext_ffi::load_arg(index) };
    if arg_size >= 0 {
        Some(arg_size as usize)
    } else {
        None
    }
}

/// Return the i-th argument passed to the host for the current module
/// invocation. Note that this is only relevant to contracts stored on-chain
/// since a contract deployed directly is not invoked with any arguments.
pub fn get_arg<T: FromBytes>(i: u32) -> Option<Result<T, bytesrepr::Error>> {
    let arg_size = load_arg(i)?;
    let arg_data = read_host_buffer_count(arg_size).unwrap_or_revert();
    Some(deserialize(&arg_data))
}

/// Returns caller of current context.
/// When in root context (not in the sub call) - returns None.
/// When in the sub call - returns public key of the account that made the
/// deploy.
pub fn get_caller() -> PublicKey {
    //  TODO: Once `PUBLIC_KEY_SIZE` is fixed, replace 36 with it.
    let dest_ptr = alloc_bytes(36);
    unsafe { ext_ffi::get_caller(dest_ptr) };
    let bytes = unsafe { Vec::from_raw_parts(dest_ptr, 36, 36) };
    deserialize(&bytes).unwrap_or_revert()
}

pub fn get_blocktime() -> BlockTime {
    let dest_ptr = alloc_bytes(BLOCKTIME_SER_SIZE);
    let bytes = unsafe {
        ext_ffi::get_blocktime(dest_ptr);
        Vec::from_raw_parts(dest_ptr, BLOCKTIME_SER_SIZE, BLOCKTIME_SER_SIZE)
    };
    deserialize(&bytes).unwrap_or_revert()
}

pub fn get_phase() -> Phase {
    let dest_ptr = alloc_bytes(PHASE_SIZE);
    unsafe { ext_ffi::get_phase(dest_ptr) };
    let bytes = unsafe { Vec::from_raw_parts(dest_ptr, PHASE_SIZE, PHASE_SIZE) };
    deserialize(&bytes).unwrap_or_revert()
}

/// Return the unforgable reference known by the current module under the given
/// name. This either comes from the named_keys of the account or contract,
/// depending on whether the current module is a sub-call or not.
pub fn get_key(name: &str) -> Option<Key> {
    let (name_ptr, name_size, _bytes) = str_ref_to_ptr(name);
    let key_size = unsafe { ext_ffi::get_key(name_ptr, name_size) };
    let key_data = read_host_buffer_count(key_size).unwrap_or_revert();
    // TODO: better error handling (i.e. pass the `Result` on)
    deserialize(&key_data).unwrap_or_revert()
}

/// Check if the given name corresponds to a known unforgable reference
pub fn has_key(name: &str) -> bool {
    let (name_ptr, name_size, _bytes) = str_ref_to_ptr(name);
    let result = unsafe { ext_ffi::has_key(name_ptr, name_size) };
    result == 0
}

/// Put the given key to the named_keys map under the given name
pub fn put_key(name: &str, key: &Key) {
    let (name_ptr, name_size, _bytes) = str_ref_to_ptr(name);
    let (key_ptr, key_size, _bytes2) = to_ptr(key);
    unsafe { ext_ffi::put_key(name_ptr, name_size, key_ptr, key_size) };
}

/// Removes Key persisted under [name] in the current context's map.
pub fn remove_key(name: &str) {
    let (name_ptr, name_size, _bytes) = str_ref_to_ptr(name);
    unsafe { ext_ffi::remove_key(name_ptr, name_size) }
}

pub fn list_named_keys() -> BTreeMap<String, Key> {
    let (total_keys, result_size) = {
        let mut total_keys = MaybeUninit::uninit();
        let mut result_size = 0;
        let ret = unsafe {
            ext_ffi::serialize_named_keys(total_keys.as_mut_ptr(), &mut result_size as *mut usize)
        };
        result_from(ret).unwrap_or_revert();
        let total_keys = unsafe { total_keys.assume_init() };
        (total_keys, result_size)
    };
    if total_keys == 0 {
        return BTreeMap::new();
    }
    let bytes = read_host_buffer_count(result_size).unwrap_or_revert();
    deserialize(&bytes).unwrap_or_revert()
}

/// Checks if all the keys contained in the given `Value`
/// (rather, thing that can be turned into a `Value`) are
/// valid, in the sense that all of the urefs (and their access rights)
/// are known in the current context.
#[allow(clippy::ptr_arg)]
pub fn is_valid<T: Into<Value>>(t: T) -> bool {
    let value = t.into();
    let (value_ptr, value_size, _bytes) = to_ptr(&value);
    let result = unsafe { ext_ffi::is_valid(value_ptr, value_size) };
    result != 0
}

pub fn read_host_buffer_into(dest: &mut [u8]) -> Result<usize, Error> {
    let mut bytes_written = MaybeUninit::uninit();
    let ret = unsafe {
        ext_ffi::read_host_buffer(dest.as_mut_ptr(), dest.len(), bytes_written.as_mut_ptr())
    };
    result_from(ret).map(|_| unsafe { bytes_written.assume_init() })
}

pub fn read_host_buffer_count(size: usize) -> Result<Vec<u8>, Error> {
    let bytes_ptr = alloc_bytes(size);
    let mut dest: Vec<u8> = unsafe { Vec::from_raw_parts(bytes_ptr, size, size) };
    read_host_buffer_into(&mut dest)?;
    Ok(dest)
}
