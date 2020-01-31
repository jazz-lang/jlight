pub mod arc;
pub mod deref_ptr;
pub mod ptr;
pub mod tagged_pointer;

#[macro_export]
macro_rules! map {
    (ahash $($key: expr => $value: expr),*) => {
        {
            use ahash::AHashMap;
            let mut map = AHashMap::new();
            $(
                map.insert($key, $value);
            )*
            map
        }
    };
}
