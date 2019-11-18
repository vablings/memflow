use crate::error::Result;

use crate::win::{Windows, types::PDB};
use crate::kernel::StartBlock;

use flow_core::address::{Address, Length};
use flow_core::mem::{VirtualRead};

use std::rc::Rc;
use std::cell::RefCell;

pub struct ProcessIterator<'a, T: VirtualRead> {
    win: &'a mut Windows<T>,
    eprocess: Address,
}

impl<'a, T: VirtualRead> ProcessIterator<'a, T> {
    pub fn new(win: &'a mut Windows<T>) -> Self {
        let eprocess = win.eprocess_base;
        ProcessIterator{
            win: win,
            eprocess: eprocess,
        }
    }
}

impl<'a, T: VirtualRead> Iterator for ProcessIterator<'a, T> {
    type Item = Process<T>;

    fn next(&mut self) -> Option<Process<T>> {
        // is eprocess null (first iter, read err, sysproc)?
        if self.eprocess.is_null() {
            return None;
        }

        // copy memory for the lifetime of this function
        let memcp = self.win.mem.clone();
        let memory = &mut memcp.borrow_mut();

        // resolve offsets
        let _eprocess = self.win.kernel_pdb.clone()?.get_struct("_EPROCESS")?;
        let _list_entry = self.win.kernel_pdb.clone()?.get_struct("_LIST_ENTRY")?;

        let _eprocess_links = _eprocess.get_field("ActiveProcessLinks")?.offset;
        let _list_entry_blink = _list_entry.get_field("Blink")?.offset;

        // read next eprocess entry
        let mut next = memory.virt_read_addr(
            self.win.start_block.arch,
            self.win.start_block.dtb,
            self.eprocess + _eprocess_links + _list_entry_blink).unwrap(); // TODO: convert to Option
        if !next.is_null() {
            next -= _eprocess_links;
        }
    
        // if next process is 'system' again just null it
        if next == self.win.eprocess_base {
            next = Address::null();
        }

        // return the previous process and set 'next' for next iter
        let cur = self.eprocess;
        self.eprocess = next;

        Some(Process::new(self.win, cur))
    }
}

pub struct Process<T: VirtualRead> {
    pub mem: Rc<RefCell<T>>,
    pub start_block: StartBlock,
    pub kernel_pdb: Option<PDB>,
    pub eprocess: Address,
}

// TODO: read/ret "ProcessInfo"
impl<T: VirtualRead> Process<T> {
    pub fn new(win: &Windows<T>, eprocess: Address) -> Self {
        Process{
            mem: win.mem.clone(),
            start_block: win.start_block,
            kernel_pdb: win.kernel_pdb.clone(), // TODO: refcell + shared access?
            eprocess: eprocess,
        }
    }

    pub fn get_pid(&mut self) -> Result<i32> {
        // TODO: remove boilerplate code?
        let memory = &mut self.mem.borrow_mut();

        let mut _pdb = self.kernel_pdb.as_mut().ok_or_else(|| "kernel pdb not found")?;
        let _eprocess = _pdb.get_struct("_EPROCESS").ok_or_else(|| "_EPROCESS not found")?;
        let _eprocess_pid = _eprocess.get_field("UniqueProcessId").ok_or_else(|| "UniqueProcessId not found")?.offset;

        Ok(memory.virt_read_i32(
            self.start_block.arch,
            self.start_block.dtb,
            self.eprocess + Length::from(_eprocess_pid))?)
    }

    pub fn get_name(&mut self) -> Result<String> {
        // TODO: remove boilerplate code?
        let memory = &mut self.mem.borrow_mut();

        let mut _pdb = self.kernel_pdb.as_mut().ok_or_else(|| "kernel pdb not found")?;
        let _eprocess = _pdb.get_struct("_EPROCESS").ok_or_else(|| "_EPROCESS not found")?;
        let _eprocess_name = _eprocess.get_field("ImageFileName").ok_or_else(|| "ImageFileName not found")?.offset;

        Ok(memory.virt_read_cstr(
            self.start_block.arch,
            self.start_block.dtb,
            self.eprocess + Length::from(_eprocess_name),
            Length::from(16))?)
    }
}
