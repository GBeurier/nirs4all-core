#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    // The campaign's -max_len is a harness policy, not an intrinsic V1 limit.
    // One runtime-free production boundary: neutral storage plus selected DAG closure.
    let _ = nirs4all::n4a_native_package::load_native_package_bytes(
        bytes, None, nirs4all::n4a::N4aMemoryBudget::host_allocator(),
    );
});
