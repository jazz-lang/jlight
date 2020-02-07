use crate::util::shared::{Arc, Mutex};
use fxhash::FxHashMap;
use std::{borrow::Borrow, fmt, ops::Deref};

lazy_static::lazy_static! {
    pub static ref INTERNER: Interner = Interner::new();
}

/// Get `Name` from string value
#[inline]
pub fn intern(name: &str) -> Name {
    INTERNER.intern(name)
}

/// Get string value from interned name
#[inline]
pub fn str(name: Name) -> ArcStr {
    INTERNER.str(name)
}

/// This struct represents interned strings
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct Name(pub usize);

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Name({},{})", str(*self), self.0)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", str(*self))
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, x: &str) -> bool {
        let val = str(*self);
        *val == x
    }
}

impl PartialEq<Name> for str {
    fn eq(&self, x: &Name) -> bool {
        let val = str(*x);
        *val == self
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, x: &&str) -> bool {
        let val = str(*self);
        *val == *x
    }
}

impl PartialEq<&Name> for str {
    fn eq(&self, x: &&Name) -> bool {
        let val = str(**x);
        *val == self
    }
}

/// ArcStr used to send string through threads safely
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ArcStr(pub Arc<String>);

impl fmt::Display for ArcStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", &*self.0)
    }
}

impl fmt::Debug for ArcStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", &*self.0)
    }
}

impl ArcStr {
    fn new(value: String) -> ArcStr {
        ArcStr(Arc::new(value))
    }
}

impl Borrow<str> for ArcStr {
    fn borrow(&self) -> &str {
        &self.0[..]
    }
}

impl Deref for ArcStr {
    type Target = String;

    fn deref(&self) -> &String {
        &self.0
    }
}

pub struct Interner {
    data: Mutex<Internal>,
}

impl Default for Interner {
    fn default() -> Self {
        Self::new()
    }
}

struct Internal {
    map: FxHashMap<ArcStr, Name>,
    vec: Vec<ArcStr>,
}

impl Interner {
    /// Create new interner
    pub fn new() -> Interner {
        Interner {
            data: Mutex::new(Internal {
                map: FxHashMap::with_hasher(fxhash::FxBuildHasher::default()),
                vec: Vec::new(),
            }),
        }
    }
    /// Intern string
    pub fn intern(&self, name: &str) -> Name {
        let mut data = self.data.lock();

        if let Some(&val) = data.map.get(name) {
            return val;
        }

        let key = ArcStr::new(String::from(name));
        let value = Name(data.vec.len());

        data.vec.push(key.clone());
        data.map.insert(key, value);

        value
    }
    /// Get string from interned name
    pub fn str(&self, name: Name) -> ArcStr {
        let data = self.data.lock();
        data.vec[name.0].clone()
    }
}
