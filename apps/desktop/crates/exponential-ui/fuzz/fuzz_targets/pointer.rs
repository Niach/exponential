//! JSON pointer read / write on arbitrary data: a write either lands (and
//! reads back) or is refused with a reason; never a panic, never a
//! partially written tree.
#![no_main]

use arbitrary::Arbitrary;
use exponential_ui::data::{get_pointer, pointer_tokens, set_pointer, write_tokens};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    data: String,
    pointer: String,
    value: Option<String>,
}

fuzz_target!(|input: Input| {
    let mut data = serde_json::from_str::<serde_json::Value>(&input.data).unwrap_or(serde_json::Value::Null);
    let before = data.clone();
    let _ = pointer_tokens(&input.pointer);
    let _ = get_pointer(&data, &input.pointer);
    let value = input.value.as_deref().map(|v| serde_json::from_str::<serde_json::Value>(v).unwrap_or(serde_json::Value::String(v.to_string())));
    let tokens = write_tokens(&input.pointer);
    match set_pointer(&mut data, &input.pointer, value.clone()) {
        Ok(()) => {
            if let (Ok(_), Some(v)) = (&tokens, &value) {
                if !input.pointer.is_empty() {
                    assert_eq!(get_pointer(&data, &input.pointer), Some(v), "a landed write reads back");
                }
            }
        }
        Err(_) => assert_eq!(data, before, "a refused write changes nothing"),
    }
});
