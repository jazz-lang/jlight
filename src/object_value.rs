use crate::object::*;
use crate::sync::Arc;
use num_bigint::BigInt;
use std::fs;
use std::mem;

pub struct Function {
    /// function name
    pub name: Arc<String>,
    /// captured values from parent scope
    pub upvalues: Vec<ObjectPointer>,
    /// Argument count.
    /// function entrypoint
    pub block: u16,
    /// Native function pointer
    pub native: Option<extern "C" fn(ObjectPointer, &[ObjectPointer]) -> ObjectPointer>,
}

pub enum ObjectValue {
    None,
    Number(f64),
    Bool(bool),
    String(Arc<String>),
    File(Box<fs::File>),
    Array(Box<Vec<ObjectPointer>>),
    ByteArray(Box<Vec<u8>>),
    BigInt(Box<BigInt>),
    Function(Box<Function>),
    Process(crate::process::RcProcess),
    Hasher(Box<crate::hasher::Hasher>),
    Module(Arc<crate::module::Module>),
}

impl ObjectValue {
    pub fn is_none(&self) -> bool {
        match *self {
            ObjectValue::None => true,
            _ => false,
        }
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn is_number(&self) -> bool {
        match *self {
            ObjectValue::Number(_) => true,
            _ => false,
        }
    }

    pub fn is_function(&self) -> bool {
        match *self {
            ObjectValue::Function(_) => true,
            _ => false,
        }
    }

    pub fn is_array(&self) -> bool {
        match *self {
            ObjectValue::Array(_) => true,
            _ => false,
        }
    }

    pub fn is_string(&self) -> bool {
        match *self {
            ObjectValue::String(_) => true,
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        match *self {
            ObjectValue::File(_) => true,
            _ => false,
        }
    }

    pub fn is_bigint(&self) -> bool {
        match *self {
            ObjectValue::BigInt(_) => true,
            _ => false,
        }
    }

    pub fn as_number(&self) -> Result<f64, String> {
        match *self {
            ObjectValue::Number(val) => Ok(val),
            _ => Err("as_float called non a non float value".to_string()),
        }
    }

    pub fn as_function(&self) -> Result<&Function, String> {
        match *self {
            ObjectValue::Function(ref val) => Ok(val),
            _ => Err("as_function called non a non function value".to_string()),
        }
    }
    pub fn as_function_mut(&mut self) -> Result<&mut Function, String> {
        match *self {
            ObjectValue::Function(ref mut val) => Ok(val),
            _ => Err("as_function called non a non function value".to_string()),
        }
    }

    pub fn as_array(&self) -> Result<&Vec<ObjectPointer>, String> {
        match *self {
            ObjectValue::Array(ref val) => Ok(val),
            _ => Err("as_array called non a non array value".to_string()),
        }
    }

    pub fn as_array_mut(&mut self) -> Result<&mut Vec<ObjectPointer>, String> {
        match *self {
            ObjectValue::Array(ref mut val) => Ok(val),
            _ => Err("as_array_mut called on a non array".to_string()),
        }
    }

    pub fn as_byte_array(&self) -> Result<&Vec<u8>, String> {
        match *self {
            ObjectValue::ByteArray(ref val) => Ok(val),
            _ => Err("as_byte_array called non a non byte array value".to_string()),
        }
    }

    pub fn as_byte_array_mut(&mut self) -> Result<&mut Vec<u8>, String> {
        match *self {
            ObjectValue::ByteArray(ref mut val) => Ok(val),
            _ => Err("as_byte_array_mut called on a non byte array".to_string()),
        }
    }

    pub fn as_string(&self) -> Result<&String, String> {
        match *self {
            ObjectValue::String(ref val) => Ok(val),
            _ => Err("ObjectValue::as_string() called on a non string".to_string()),
        }
    }

    pub fn as_file(&self) -> Result<&fs::File, String> {
        match *self {
            ObjectValue::File(ref val) => Ok(val),
            _ => Err("ObjectValue::as_file() called on a non file".to_string()),
        }
    }

    pub fn as_file_mut(&mut self) -> Result<&mut fs::File, String> {
        match *self {
            ObjectValue::File(ref mut val) => Ok(val),
            _ => Err("ObjectValue::as_file_mut() called on a non file".to_string()),
        }
    }

    pub fn as_bigint(&self) -> Result<&BigInt, String> {
        match *self {
            ObjectValue::BigInt(ref val) => Ok(val),
            _ => Err("ObjectValue::as_bigint() called on a non BigInt".to_string()),
        }
    }

    pub fn take(&mut self) -> ObjectValue {
        mem::replace(self, ObjectValue::None)
    }

    /// Returns true if this value should be deallocated explicitly.
    pub fn should_deallocate_native(&self) -> bool {
        match *self {
            ObjectValue::None => false,
            _ => true,
        }
    }

    pub fn is_immutable(&self) -> bool {
        match *self {
            ObjectValue::Number(_) | ObjectValue::String(_) | ObjectValue::BigInt(_) => true,
            _ => false,
        }
    }

    pub fn is_process(&self) -> bool {
        match *self {
            ObjectValue::Process(_) => true,
            _ => false,
        }
    }
    pub fn as_process(&self) -> Result<&crate::process::Process, String> {
        match *self {
            ObjectValue::Process(ref val) => Ok(val),
            _ => Err("ObjectValue::as_process() called on a non Process".to_string()),
        }
    }
    pub fn as_process_mut(&mut self) -> Result<&mut crate::process::Process, String> {
        match *self {
            ObjectValue::Process(ref mut val) => Ok(val),
            _ => Err("ObjectValue::as_process_mut() called on a non Process".to_string()),
        }
    }
    pub fn name(&self) -> &str {
        match *self {
            ObjectValue::None => "Object",
            ObjectValue::Number(_) => "Number",
            ObjectValue::String(_) => "String",
            ObjectValue::Array(_) => "Array",
            ObjectValue::File(_) => "File",
            ObjectValue::BigInt(_) => "BigInteger",
            ObjectValue::Bool(_) => "Boolean",
            ObjectValue::ByteArray(_) => "ByteArray",
            ObjectValue::Function { .. } => "Function",
            ObjectValue::Process(_) => "Process",
            ObjectValue::Hasher(_) => "Hasher",
            ObjectValue::Module(_) => "Module",
        }
    }
}
