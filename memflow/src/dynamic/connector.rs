use crate::error::*;
use crate::mem::{PhysicalMemory, PhysicalMemoryMetadata, PhysicalReadData, PhysicalWriteData};

use super::{Args, LibInstance, Loadable};

use std::ffi::{c_void, CString};
use std::os::raw::c_char;
use std::sync::Arc;

use libloading::Library;

use log::*;

/// Exported memflow connector version
pub const MEMFLOW_CONNECTOR_VERSION: i32 = 8;

/// Describes a connector
#[repr(C)]
pub struct ConnectorDescriptor {
    /// The connector inventory api version for when the connector was built.
    /// This has to be set to `MEMFLOW_CONNECTOR_VERSION` of memflow.
    ///
    /// If the versions mismatch the inventory will refuse to load.
    pub connector_version: i32,

    /// The name of the connector.
    /// This name will be used when loading a connector from a connector inventory.
    pub name: &'static str,

    /// The vtable for all opaque function calls to the connector.
    pub vtable: ConnectorFunctionTable,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConnectorFunctionTable {
    /// The vtable for object creation and cloning
    pub base: ConnectorBaseTable,

    /// The vtable for all physical memory funmction calls to the connector.
    pub phys: PhysicalMemoryFunctionTable,
    // further optional table expansion with Option<&'static SomeFunctionTable>
    // ...
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConnectorBaseTable {
    pub create: extern "C" fn(args: *const c_char, log_level: i32) -> Option<&'static mut c_void>,

    pub clone: extern "C" fn(phys_mem: &c_void) -> Option<&'static mut c_void>,

    pub drop: extern "C" fn(phys_mem: &mut c_void),
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PhysicalMemoryFunctionTable {
    pub phys_read_raw_list: extern "C" fn(
        phys_mem: &mut c_void,
        read_data: *mut PhysicalReadData,
        read_data_count: usize,
    ) -> i32,
    pub phys_write_raw_list: extern "C" fn(
        phys_mem: &mut c_void,
        write_data: *const PhysicalWriteData,
        write_data_count: usize,
    ) -> i32,
    pub metadata: extern "C" fn(phys_mem: &c_void) -> PhysicalMemoryMetadata,
}

/// Describes initialized connector instance
///
/// This structure is returned by `Connector`. It is needed to maintain reference
/// counts to the loaded connector library.
pub struct ConnectorInstance {
    instance: &'static mut c_void,
    vtable: ConnectorFunctionTable,

    /// Internal library arc.
    ///
    /// This will keep the library loaded in memory as long as the connector instance is alive.
    /// This has to be the last member of the struct so the library will be unloaded _after_
    /// the instance is destroyed.
    ///
    /// If the library is unloaded prior to the instance this will lead to a SIGSEGV.
    library: Arc<Library>,
}

impl PhysicalMemory for ConnectorInstance {
    fn phys_read_raw_list(&mut self, data: &mut [PhysicalReadData]) -> Result<()> {
        (self.vtable.phys.phys_read_raw_list)(self.instance, data.as_mut_ptr(), data.len());
        Ok(())
    }

    fn phys_write_raw_list(&mut self, data: &[PhysicalWriteData]) -> Result<()> {
        (self.vtable.phys.phys_write_raw_list)(
            self.instance,
            data.as_ptr() as *mut PhysicalWriteData,
            data.len(),
        );
        Ok(())
    }

    fn metadata(&self) -> PhysicalMemoryMetadata {
        (self.vtable.phys.metadata)(self.instance)
    }
}

impl Clone for ConnectorInstance {
    fn clone(&self) -> Self {
        let instance = (self.vtable.base.clone)(self.instance).expect("Unable to clone Connector");
        Self {
            instance,
            vtable: self.vtable.clone(),
            library: self.library.clone(),
        }
    }
}

impl Drop for ConnectorInstance {
    fn drop(&mut self) {
        (self.vtable.base.drop)(self.instance);
    }
}

pub struct LoadableConnector {
    descriptor: ConnectorDescriptor,
}

impl Loadable for LoadableConnector {
    type Instance = ConnectorInstance;

    fn ident(&self) -> &str {
        self.descriptor.name
    }

    unsafe fn load(library: Library) -> Result<LibInstance<Self>> {
        let descriptor = library
            .get::<*mut ConnectorDescriptor>(b"MEMFLOW_CONNECTOR\0")
            .map_err(|_| Error::Connector("connector descriptor not found"))?
            .read();

        if descriptor.connector_version != MEMFLOW_CONNECTOR_VERSION {
            warn!(
                "connector {:?} has a different version. version {} required, found {}.",
                "PATHHOLDER", MEMFLOW_CONNECTOR_VERSION, descriptor.connector_version
            );
            return Err(Error::Connector("connector version mismatch"));
        }

        Ok(LibInstance {
            library: Arc::new(library),
            loader: LoadableConnector { descriptor },
        })
    }

    /// Creates a new connector instance from this library.
    ///
    /// The connector is initialized with the arguments provided to this function.
    fn instantiate(&self, lib: Arc<Library>, args: &Args) -> Result<ConnectorInstance> {
        let cstr = CString::new(args.to_string())
            .map_err(|_| Error::Connector("args could not be parsed"))?;

        // We do not want to return error with data from the shared library
        // that may get unloaded before it gets displayed
        let instance = (self.descriptor.vtable.base.create)(cstr.as_ptr(), log::max_level() as i32)
            .ok_or(Error::Connector("create() failed"))?;

        Ok(ConnectorInstance {
            instance,
            vtable: self.descriptor.vtable,
            library: lib.clone(),
        })
    }
}

pub type ConnectorInventory = super::LibInventory<LoadableConnector>;
