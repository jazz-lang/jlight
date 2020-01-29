use crate::bytecode::*;
use crate::object::*;
use crate::object_value::*;
use crate::ptr::*;
use crate::state::*;
use crate::sync::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;

/// A module is a single file containing bytecode and an associated global
/// scope.
pub struct Module {
    pub name: ObjectPointer,
    pub path: ObjectPointer,
    pub code: Vec<BasicBlock>,
    pub globals: Ptr<Vec<ObjectPointer>>,
}

impl Module {
    pub fn new(name: ObjectPointer, path: ObjectPointer, code: Vec<BasicBlock>) -> Self {
        Self {
            name,
            path,
            code,
            globals: Ptr::new(vec![]),
        }
    }
    pub fn name(&self) -> ObjectPointer {
        self.name
    }

    pub fn path(&self) -> ObjectPointer {
        self.path
    }

    pub fn global_scope(&self) -> Ptr<Vec<ObjectPointer>> {
        self.globals
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        self.name.finalize();
        self.path.finalize();
        unsafe {
            std::ptr::drop_in_place(self.globals.0);
        }
    }
}

// A module is immutable once created. The lines below ensure we can store a
// Module in ModuleRegistry without needing a special Sync/Send type (e.g. Arc).
unsafe impl Sync for Module {}
unsafe impl Send for Module {}

pub type RcModuleRegistry = Arc<Mutex<ModuleRegistry>>;
pub struct ModuleRegistry {
    state: RcState,

    /// Mapping of the module names parsed thus far and their Module objects.
    parsed: HashMap<String, ObjectPointer>,
}

pub enum ModuleError {
    /// The module did exist but could not be parsed.
    FailedToParse(String),

    /// A given module did not exist.
    ModuleDoesNotExist(String),
}

impl ModuleError {
    /// Returns a human friendly error message.
    pub fn message(&self) -> String {
        match *self {
            ModuleError::FailedToParse(ref path) => format!("Failed to parse {}", path),
            ModuleError::ModuleDoesNotExist(ref path) => format!("Module does not exist: {}", path),
        }
    }
}

impl ModuleRegistry {
    pub fn with_rc(state: RcState) -> RcModuleRegistry {
        Arc::new(Mutex::new(ModuleRegistry::new(state)))
    }

    pub fn new(state: RcState) -> Self {
        ModuleRegistry {
            state,
            parsed: HashMap::new(),
        }
    }

    /// Returns true if the given module has been parsed.
    #[cfg_attr(feature = "cargo-clippy", allow(ptr_arg))]
    pub fn contains(&self, name: &str) -> bool {
        self.parsed.contains_key(name)
    }

    /// Returns all parsed modules.
    pub fn parsed(&self) -> Vec<ObjectPointer> {
        self.parsed.values().copied().collect()
    }

    /// Obtains a parsed module by its name.
    pub fn get(&self, name: &str) -> Option<ObjectPointer> {
        self.parsed.get(name).copied()
    }

    /// Returns the full path for a relative path.
    fn find_path(&self, path: &str) -> Result<String, ModuleError> {
        let mut input_path = PathBuf::from(path);

        if input_path.is_relative() {
            let mut found = false;

            for directory in &self.state.config.directories {
                let full_path = directory.join(path);

                if full_path.exists() {
                    input_path = full_path;
                    found = true;

                    break;
                }
            }

            if !found {
                return Err(ModuleError::ModuleDoesNotExist(path.to_string()));
            }
        }

        Ok(input_path.to_str().unwrap().to_string())
    }

    /// Parses a full file path pointing to a module.
    pub fn parse_module(&mut self, name: &str, path: &str) -> Result<ObjectPointer, ModuleError> {
        /*let code = bytecode_parser::parse_file(&self.state, path)
            .map_err(|err| ModuleError::FailedToParse(path.to_string(), err))?;

        Ok(self.define_module(name, path, code))*/

        unimplemented!()
    }

    pub fn define_module(
        &mut self,
        name: &str,
        path: &str,
        code: Vec<crate::bytecode::BasicBlock>,
    ) -> ObjectPointer {
        let name_obj = self.state.intern_string(name.to_string());
        let path_obj = self.state.intern_string(path.to_string());

        let module_val = ObjectValue::Module(Arc::new(Module::new(name_obj, path_obj, code)));

        let prototype = self.state.module_prototype;
        let module = self
            .state
            .permanent_allocator
            .lock()
            .allocate_with_prototype(module_val, prototype);

        self.parsed.insert(name.to_string(), module);

        module
    }
}
