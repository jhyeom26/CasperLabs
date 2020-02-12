#![no_std]

extern crate alloc;

use alloc::{collections::BTreeMap, string::String};

use contract_ffi::{
    contract_api::{runtime, storage, ContractRef, Error as ApiError, TURef},
    key::Key,
    unwrap_or_revert::UnwrapOrRevert,
    value::CLValue,
};

const COUNT_KEY: &str = "count";
const COUNTER_EXT: &str = "counter_ext";
const COUNTER_KEY: &str = "counter";
const GET_METHOD: &str = "get";
const INC_METHOD: &str = "inc";

const COUNTER_PROXY_NAME: &str = "counter_proxy";

enum Arg {
    MethodName = 0,
}

#[repr(u16)]
enum Error {
    UnknownMethodName = 0,
}

impl Into<ApiError> for Error {
    fn into(self) -> ApiError {
        ApiError::User(self as u16)
    }
}

#[no_mangle]
pub extern "C" fn counter_ext() {
    let turef: TURef<i32> = runtime::get_key(COUNT_KEY)
        .unwrap_or_revert()
        .to_turef()
        .unwrap_or_revert_with(ApiError::UnexpectedKeyVariant);

    let method_name: String = runtime::get_arg(Arg::MethodName as u32)
        .unwrap_or_revert_with(ApiError::MissingArgument)
        .unwrap_or_revert_with(ApiError::InvalidArgument);

    match method_name.as_str() {
        INC_METHOD => storage::add(turef, 1),
        GET_METHOD => {
            let result = storage::read(turef)
                .unwrap_or_revert_with(ApiError::Read)
                .unwrap_or_revert_with(ApiError::ValueNotFound);
            let return_value = CLValue::from_t(result).unwrap_or_revert();
            runtime::ret(return_value);
        }
        _ => runtime::revert(Error::UnknownMethodName),
    }
}

fn deploy_counter() {
    let counter_local_key = storage::new_turef(0); //initialize counter

    //create map of references for stored contract
    let mut counter_urefs: BTreeMap<String, Key> = BTreeMap::new();
    let key_name = String::from(COUNT_KEY);
    counter_urefs.insert(key_name, counter_local_key.into());

    let pointer = storage::store_function(COUNTER_EXT, counter_urefs);
    runtime::put_key(COUNTER_KEY, pointer.into());
}

fn deploy_proxy() {
    // Create proxy instance.
    let proxy_ref = storage::store_function(COUNTER_PROXY_NAME, Default::default());
    runtime::put_key(COUNTER_PROXY_NAME, proxy_ref.into());
}

#[no_mangle]
pub extern "C" fn counter_proxy() {
    let counter_uref = match runtime::get_arg::<Key>(0)
        .unwrap_or_revert_with(ApiError::MissingArgument)
        .unwrap_or_revert_with(ApiError::InvalidArgument)
    {
        Key::URef(uref) => uref,
        _ => runtime::revert(ApiError::InvalidArgument),
    };

    let counter_contract = ContractRef::URef(counter_uref);
    let method_name = runtime::get_arg::<String>(1)
        .unwrap_or_revert_with(ApiError::MissingArgument)
        .unwrap_or_revert_with(ApiError::InvalidArgument);

    match method_name.as_ref() {
        GET_METHOD => runtime::call_contract(counter_contract.clone(), (GET_METHOD,)),
        INC_METHOD => runtime::call_contract(counter_contract.clone(), (INC_METHOD,)),
        _ => runtime::revert(Error::UnknownMethodName),
    };
}

#[no_mangle]
pub extern "C" fn call() {
    deploy_counter();
    deploy_proxy();
}
