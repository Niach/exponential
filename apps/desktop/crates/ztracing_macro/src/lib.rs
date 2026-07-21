use proc_macro::TokenStream;

/// No-op `#[instrument]`: accepts any arguments and emits the item unchanged.
#[proc_macro_attribute]
pub fn instrument(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}
