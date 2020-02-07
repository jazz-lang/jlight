#[macro_use]
extern crate log;
#[macro_use]
pub mod util;
pub mod bytecode;
pub mod heap;
pub mod runtime;

cfg_if::cfg_if! {
    if #[cfg(debug_assertions)] {
        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
    } else {
        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
    }
}
