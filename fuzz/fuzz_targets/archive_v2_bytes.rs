#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > MAX_INPUT_BYTES {
        return;
    }

    // Exercise the canonical in-memory Archive V2 parser and validator. Keeping
    // the target at this public boundary avoids a second ZIP/manifest parser and
    // cannot invoke the Python binding.
    let _ = nirs4all::load_archive_v2_bytes(bytes);
});
