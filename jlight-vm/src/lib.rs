#[macro_use]
pub mod util;
pub mod bytecode;
pub mod heap;
pub mod runtime;
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
