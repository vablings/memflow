use crate::error::Result;

use super::PageCache;

use crate::address::{Address, Page, PhysicalAddress};
use crate::arch::Architecture;
use crate::mem::{AccessPhysicalMemory, AccessVirtualMemory};
use crate::page_chunks::{PageChunks, PageChunksMut};
use crate::vat;

// TODO: derive virtual reads here
pub struct CachedMemoryAccess<'a, T: AccessPhysicalMemory> {
    mem: &'a mut T,
    cache: &'a mut dyn PageCache,
}

impl<'a, T: AccessPhysicalMemory> CachedMemoryAccess<'a, T> {
    pub fn with(mem: &'a mut T, cache: &'a mut dyn PageCache) -> Self {
        Self { mem, cache }
    }
}

// TODO: calling phys_read_raw_into non page alligned causes UB
// forward AccessPhysicalMemory trait fncs
impl<'a, T: AccessPhysicalMemory> AccessPhysicalMemory for CachedMemoryAccess<'a, T> {
    fn phys_read_raw_into(&mut self, addr: PhysicalAddress, out: &mut [u8]) -> Result<()> {
        if let Some(page) = addr.page {
            // try read from cache or fall back
            if self.cache.is_cached_page_type(page.page_type) {
                for (paddr, chunk) in PageChunksMut::create_from(
                    out,
                    addr.address,
                    self.cache.page_size(), /* std::cmp::min(page.page_size, self.cache.page_size())*/
                ) {
                    let cached_page = self.cache.cached_page_mut(paddr);
                    // read into page buffer and set addr
                    if !cached_page.is_valid() {
                        self.mem
                            .phys_read_raw_into(cached_page.address.into(), cached_page.buf)?;
                    }

                    // copy page into out buffer
                    // TODO: reowkr this logic, no comptuations needed
                    let start = (paddr - cached_page.address).as_usize();
                    chunk.copy_from_slice(&cached_page.buf[start..(start + chunk.len())]);

                    // update update page if it wasnt valid before
                    // this is done here due to borrowing constraints
                    if !cached_page.is_valid() {
                        self.cache.validate_page(paddr, page.page_type);
                    }
                }
            } else {
                self.mem.phys_read_raw_into(addr, out)?
            }
        } else {
            // page is not cacheable (no page info)
            self.mem.phys_read_raw_into(addr, out)?;
        }

        Ok(())
    }

    fn phys_write_raw(&mut self, addr: PhysicalAddress, data: &[u8]) -> Result<()> {
        // TODO: implement writeback to cache
        if let Some(page) = addr.page {
            for (paddr, _) in PageChunks::create_from(data, addr.address, self.cache.page_size()) {
                self.cache.invalidate_page(paddr, page.page_type);
            }
        }
        self.mem.phys_write_raw(addr, data)
    }
}

// forward AccessVirtualMemory trait fncs if memory has them implemented
impl<'a, T> AccessVirtualMemory for CachedMemoryAccess<'a, T>
where
    T: AccessPhysicalMemory + AccessVirtualMemory,
{
    fn virt_read_raw_into(
        &mut self,
        arch: Architecture,
        dtb: Address,
        addr: Address,
        out: &mut [u8],
    ) -> Result<()> {
        vat::virt_read_raw_into(self, arch, dtb, addr, out)
    }

    fn virt_write_raw(
        &mut self,
        arch: Architecture,
        dtb: Address,
        addr: Address,
        data: &[u8],
    ) -> Result<()> {
        vat::virt_write_raw(self, arch, dtb, addr, data)
    }

    fn virt_page_info(&mut self, arch: Architecture, dtb: Address, addr: Address) -> Result<Page> {
        vat::virt_page_info(self, arch, dtb, addr)
    }
}
