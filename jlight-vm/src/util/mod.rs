pub mod arc;
pub mod deref_ptr;
pub mod ptr;
pub mod rc;
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

pub mod shared {
    cfg_if::cfg_if!(
        if #[cfg(feature="multithreaded")] {
            pub use super::arc::Arc;
            pub use parking_lot::{RwLock,Mutex,Condvar};
        } else {
            pub use super::rc::Rc as Arc;
            use std::cell::{Ref,RefMut,RefCell};
            pub struct RwLock<T> {
                x: super::ptr::Ptr<T>
            }

            impl<T> RwLock<T> {
                pub fn new(x: T) -> Self {
                    Self {
                        x:super::ptr::Ptr::new(x)
                    }
                }

                pub fn write(&self) ->&mut T {
                    self.x.get()
                }

                pub fn read(&self) -> &T {
                    self.x.get()
                }
            }

            impl<T> Drop for RwLock<T> {
                fn drop(&mut self) {
                    unsafe {
                        let _ = Box::from_raw(self.x.0);
                    }
                }
            }

            pub struct Mutex<T> {
                x: super::ptr::Ptr<T>
            }

            impl<T> Mutex<T> {
                pub fn new(x: T) -> Self {
                    Self {
                        x: super::ptr::Ptr::new(x)
                    }
                }
                pub fn lock(&self) -> &mut T {
                    self.x.get()
                }
            }

            impl<T> Drop for Mutex<T> {
                fn drop(&mut self) {
                    unsafe {
                        let _ = Box::from_raw(self.x.0);
                    }
                }
            }

            pub struct Condvar;

            impl Condvar {
                pub const fn notify_one(&self) {

                }
                pub fn wait<T>(&self,_: &mut T) {}
                pub const fn notify_all(&self) {}
                pub const fn new() -> Self {
                    Self
                }
            }
        }
    );
}
