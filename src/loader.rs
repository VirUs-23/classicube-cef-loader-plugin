use crate::{
    cef_binary_updater::CEF_BINARY_PATH, error::*, plugin_updater::CEF_PLUGIN_PATH, print,
};
use classicube_sys::IGameComponent;
use std::{cell::Cell, env, ffi::CString, io};
use winapi::{
    shared::minwindef::HMODULE,
    um::libloaderapi::{FreeLibrary, GetProcAddress, LoadLibraryA},
};

thread_local!(
    static LIBRARY: Cell<Option<HMODULE>> = Cell::new(None);
);

thread_local!(
    static PLUGIN: Cell<Option<*mut IGameComponent>> = Cell::new(None);
);

fn ptr_result<T>(ptr: *mut T) -> Result<*mut T> {
    if ptr.is_null() {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(ptr)
    }
}

fn try_init() -> Result<()> {
    // add cef/cef_binary and cef/ to PATH so that cef.dll is found,
    // and cefsimple.exe can run
    let path = env::var("PATH").unwrap();
    env::set_var("PATH", format!("{};{};{}", path, CEF_BINARY_PATH, "cef"));

    // let cef_dll_path = CString::new(CEF_BINARY_PATH).unwrap();
    // assert!(unsafe { SetDllDirectoryA(cef_dll_path.as_ptr()) } != 0);

    let dll_path = CString::new(CEF_PLUGIN_PATH).unwrap();
    let library = ptr_result(unsafe { LoadLibraryA(dll_path.as_ptr()) })?;

    LIBRARY.with(|cell| cell.set(Some(library)));

    let plugin_component_name = CString::new("Plugin_Component").unwrap();
    let plugin_component =
        ptr_result(unsafe { GetProcAddress(library, plugin_component_name.as_ptr()) })?;
    let plugin_component: *mut IGameComponent = plugin_component as _;

    PLUGIN.with(|cell| cell.set(Some(plugin_component)));

    let plugin_component = unsafe { &mut *plugin_component };

    if let Some(f) = plugin_component.Init {
        unsafe {
            f();
        }
    }

    Ok(())
}

pub fn init() {
    if let Err(e) = try_init() {
        print(format!("Couldn't load cef plugin: {}", e));
    }
}

pub fn on_new_map_loaded() {
    PLUGIN.with(|cell| {
        if let Some(plugin_component) = cell.get() {
            let plugin_component = unsafe { &mut *plugin_component };

            if let Some(f) = plugin_component.OnNewMapLoaded {
                unsafe {
                    f();
                }
            }
        }
    });
}

pub fn free() {
    PLUGIN.with(|cell| {
        if let Some(plugin_component) = cell.get() {
            cell.set(None);

            let plugin_component = unsafe { &mut *plugin_component };

            if let Some(f) = plugin_component.Free {
                unsafe {
                    f();
                }
            }
        }
    });

    LIBRARY.with(|cell| {
        if let Some(library) = cell.get() {
            cell.set(None);

            unsafe {
                FreeLibrary(library);
            }
        }
    });
}