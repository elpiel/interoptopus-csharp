use std::{
    ffi::{c_char, CStr, CString},
    ptr::null,
};

use interoptopus::{
    callback, ffi_function, ffi_service, ffi_service_ctor, ffi_type, function, pattern,
    patterns::{
        api_guard::APIVersion, option::FFIOption, slice::FFISliceMut, string::AsciiPointer,
    },
    Inventory, InventoryBuilder,
};

use crate::ffi_error::FFIError;

callback!(DebugLogCallback(debug_log: AsciiPointer));

#[derive(Debug)]
pub enum EnvError {
    Other(String),
}

#[ffi_type(opaque)]
#[repr(C)]
pub struct CoreService {
    storage: Option<StorageI>,
}

enum StorageI {
    Bare(Storage),
    Ascii(StorageAscii),
}

#[ffi_service(error = "FFIError", prefix = "core_")]
impl CoreService {
    /// Takes a Storage instance to be used in the [`CSharpEnv`] impl
    ///
    /// On panic it should return an error because of the Service impl of [`interoptopus`].
    #[ffi_service_ctor]
    pub fn initialize_native_with_debug_call(debug_callback: DebugLogCallback) -> Result<Self, EnvError> {
        debug_callback.call(AsciiPointer::from_cstr(
            CString::new("debug callback has been triggered")
                .map_err(|_| EnvError::Other("doesn't work".into()))?
                .as_c_str(),
        ));

        Ok(Self { storage: None })
    }

    #[ffi_service_ctor]
    pub fn initialize_with_storage_with_set(storage: Storage) -> Result<Self, EnvError> {
        storage.storage_set("key", Some("value".into()));

        Ok(Self {
            storage: Some(StorageI::Bare(storage)),
        })
    }

    #[ffi_service_ctor]
    pub fn initialize_with_storage_with_get(storage: Storage) -> Result<Self, EnvError> {
        let _value = storage.storage_get("key");

        Ok(Self {
            storage: Some(StorageI::Bare(storage)),
        })
    }

    #[ffi_service_ctor]
    pub fn initialize_with_storage_without_set_get(
        storage: Storage,
        debug_callback: DebugLogCallback,
    ) -> Result<Self, EnvError> {
        debug_callback.call(AsciiPointer::from_cstr(
            CString::new("debug callback has been triggered")
                .map_err(|_| EnvError::Other("doesn't work".into()))?
                .as_c_str(),
        ));

        Ok(Self {
            storage: Some(StorageI::Bare(storage)),
        })
    }

    #[ffi_service_ctor]
    pub fn initialize_with_storage_ascii_with_set(storage: StorageAscii) -> Result<Self, EnvError> {
        storage.storage_set("key", Some("value".into()));

        Ok(Self {
            storage: Some(StorageI::Ascii(storage)),
        })
    }

    #[ffi_service_ctor]
    pub fn initialize_with_storage_ascii_with_get(storage: StorageAscii) -> Result<Self, EnvError> {
        let _value = storage.storage_get("key");

        Ok(Self {
            storage: Some(StorageI::Ascii(storage)),
        })
    }

    #[ffi_service_ctor]
    pub fn initialize_with_storage_ascii_without_set_get(
        storage: StorageAscii,
        debug_callback: DebugLogCallback,
    ) -> Result<Self, EnvError> {
        debug_callback.call(AsciiPointer::from_cstr(
            CString::new("Before RUNTIME.read()")
                .map_err(|_| EnvError::Other("doesn't work".into()))?
                .as_c_str(),
        ));

        Ok(Self {
            storage: Some(StorageI::Ascii(storage)),
        })
    }
}

// Callback to C# to get a given key from Storage
callback!(GetStorageCallbackAscii(key: AsciiPointer) -> AsciiPointer<'static>);
callback!(GetStorageCallback(key: *const c_char) -> *const c_char);

// Callback to C# to set a given key in Storage with Json value
callback!(SetStorageCallbackAscii(key: AsciiPointer, value: AsciiPointer));
callback!(SetStorageCallback(key: *const c_char, value: *const c_char));

#[derive(Default)]
#[ffi_type(opaque)]
#[repr(C)]
pub struct StorageAscii {
    get_callback: GetStorageCallbackAscii,
    set_callback: SetStorageCallbackAscii,
}

#[ffi_service(error = "FFIError", prefix = "storage_ascii_")]
impl StorageAscii {
    #[ffi_service_ctor]
    pub fn new(
        get_callback: GetStorageCallbackAscii,
        set_callback: SetStorageCallbackAscii,
    ) -> Result<Self, EnvError> {
        Ok(Self {
            get_callback,
            set_callback,
        })
    }

    pub fn ffi_set(&self, key: AsciiPointer, value: AsciiPointer) -> Result<(), EnvError> {
        let value = value.as_c_str();

        self.storage_set(
            key.as_str().expect("Should be valid UTF-8"),
            value.map(|cstr| cstr.to_str().expect("Should be valid UTF-8").to_string()),
        );

        Ok(())
    }

    /// if key is empty (`null` in C#) in storage we return `None` and json will be `null`` as well
    pub fn ffi_get(
        &self,
        key: AsciiPointer,
        mut result: FFISliceMut<u8>,
        result_written: &mut u64,
    ) -> Result<(), EnvError> {
        let value = self.storage_get(key.as_str().expect("Valid UTF-8"))?;

        let json = value.unwrap_or(serde_json::to_string(&serde_json::Value::Null).unwrap());

        let json_cstring = CString::new(json).unwrap();

        result.as_slice_mut()[..json_cstring.as_bytes_with_nul().len()]
            .copy_from_slice(json_cstring.as_bytes_with_nul());
        *result_written = json_cstring.as_bytes_with_nul().len() as u64;

        Ok(())
    }
}

impl StorageAscii {
    fn storage_set(&self, key: &str, value: Option<String>) {
        let key = CString::new(key).unwrap();
        let key = AsciiPointer::from_cstr(&key);
        match value {
            Some(value) => {
                let value = CString::new(value).unwrap();
                let value = AsciiPointer::from_cstr(&value);

                self.set_callback.call(key, value)
            }
            None => self.set_callback.call(key, AsciiPointer::default()),
        };
    }

    fn storage_get(&self, key: &str) -> Result<Option<String>, EnvError> {
        let key = CString::new(key).unwrap();

        let value = self.get_callback.call(AsciiPointer::from_cstr(&key));

        value
            .as_c_str()
            .map(|cstr| {
                cstr.to_str()
                    .map(ToString::to_string)
                    .map_err(|err| EnvError::Other(err.to_string()))
            })
            .transpose()
    }
}

#[derive(Default)]
#[ffi_type(opaque)]
#[repr(C)]
pub struct Storage {
    get_callback: GetStorageCallback,
    set_callback: SetStorageCallback,
}

#[ffi_service(error = "FFIError", prefix = "storage_")]
impl Storage {
    #[ffi_service_ctor]
    pub fn new(
        get_callback: GetStorageCallback,
        set_callback: SetStorageCallback,
    ) -> Result<Self, EnvError> {
        Ok(Self {
            get_callback,
            set_callback,
        })
    }
}

impl Storage {
    fn storage_set(&self, key: &str, value: Option<String>) {
        let key = CString::new(key).unwrap();

        match value {
            Some(value) => {
                let value = CString::new(value).unwrap();

                self.set_callback.call(key.as_ptr(), value.as_ptr())
            }
            None => self.set_callback.call(key.as_ptr(), null()),
        };
    }

    /// if key is empty (`null` in C#) in storage we return `None``
    pub fn storage_get(&self, key: &str) -> Option<String> {
        let key = CString::new(key).unwrap();
        let value_ptr = self.get_callback.call(key.as_ptr());

        if value_ptr.is_null() {
            return None;
        }

        let value = unsafe { CStr::from_ptr(value_ptr).to_str().unwrap() };
        Some(value.to_string())
    }
}

pub mod ffi_error {
    use interoptopus::ffi_type;

    use crate::EnvError;

    // This is the FFI error enum you want your users to see. You are free to name and implement this
    // almost any way you want.
    #[ffi_type(patterns(ffi_error))]
    #[repr(C)]
    #[derive(Debug)]
    pub enum FFIError {
        Ok = 0,
        Null = 100,
        Panic = 200,
        Fail = 300,
    }

    // Provide a mapping how your Rust error enums translate
    // to your FFI error enums.
    impl From<EnvError> for FFIError {
        fn from(_x: EnvError) -> Self {
            Self::Fail
        }
    }

    // Implement Default so we know what the "good" case is.
    impl Default for FFIError {
        fn default() -> Self {
            Self::Ok
        }
    }

    // Implement Interoptopus' `FFIError` trait for your FFIError enum.
    // Here you must map 3 "well known" variants to your enum.
    impl interoptopus::patterns::result::FFIError for FFIError {
        const SUCCESS: Self = Self::Ok;
        const NULL: Self = Self::Null;
        const PANIC: Self = Self::Panic;
    }
}

pub fn my_inventory() -> Inventory {
    InventoryBuilder::new()
        // Register main ffi functions
        // api_guard fails on Android for some reason
        .register(function!(api_guard))
        // register Storage service
        .register(pattern!(Storage))
        .register(pattern!(StorageAscii))
        // register the Core service
        .register(pattern!(CoreService))
        .inventory()
}

#[ffi_function]
#[no_mangle]
pub extern "C" fn api_guard() -> FFIOption<APIVersion> {
    FFIOption::some(my_inventory().into())
}

#[cfg(test)]
mod test {
    use interoptopus::{util::NamespaceMappings, Error, Interop};
    use interoptopus_backend_csharp::overloads::DotNet;

    #[test]
    fn bindings_csharp() -> Result<(), Error> {
        use interoptopus_backend_csharp::{Config, Generator};

        let mut generator = Generator::new(
            Config {
                class: "CoreBindings".to_string(),
                dll_name: "basic_csharp".to_string(),
                namespace_mappings: NamespaceMappings::new("Core.CSharp.Protobuf"),
                #[cfg(feature = "unity")]
                use_unsafe: interoptopus_backend_csharp::Unsafe::UnsafeKeyword,
                ..Config::default()
            },
            super::my_inventory(),
        );
        generator.add_overload_writer(DotNet::new());

        #[cfg(feature = "unity")]
        generator.add_overload_writer(interoptopus_backend_csharp::overloads::Unity::new());

        generator.write_file("./Core.CSharp/CoreBindings.cs")?;

        Ok(())
    }
}
