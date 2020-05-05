#[cfg(test)]
mod tests;

use crate::error::{Error, Result};

use crate::address::{Address, Length, Page};
use crate::arch::Architecture;
use crate::mem::AccessPhysicalMemory;

#[allow(unused)]
pub fn virt_read_raw_into<T: AccessPhysicalMemory>(
    mem: &mut T,
    arch: Architecture,
    dtb: Address,
    addr: Address,
    out: &mut [u8],
) -> Result<()> {
    let page_size = arch.page_size();
    let aligned_len = (addr + page_size).as_page_aligned(page_size) - addr;

    if aligned_len.as_usize() >= out.len() {
        if let Ok(paddr) = arch.virt_to_phys(mem, dtb, addr) {
            mem.phys_read_raw_into(paddr, out)?;
        } else {
            for v in out.iter_mut() {
                *v = 0u8;
            }
        }
    } else {
        let mut base = addr;

        let (mut start_buf, mut end_buf) =
            out.split_at_mut(std::cmp::min(aligned_len.as_usize(), out.len()));

        for i in [start_buf, end_buf].iter_mut() {
            for chunk in i.chunks_mut(page_size.as_usize()) {
                if let Ok(paddr) = arch.virt_to_phys(mem, dtb, base) {
                    mem.phys_read_raw_into(paddr, chunk)?;
                } else {
                    for v in chunk.iter_mut() {
                        *v = 0u8;
                    }
                }
                base += Length::from(chunk.len());
            }
        }
    }

    Ok(())
}

#[allow(unused)]
pub fn virt_write_raw<T: AccessPhysicalMemory>(
    mem: &mut T,
    arch: Architecture,
    dtb: Address,
    addr: Address,
    data: &[u8],
) -> Result<()> {
    let page_size = arch.page_size();
    let aligned_len = (addr + page_size).as_page_aligned(page_size) - addr;

    if aligned_len.as_usize() >= data.len() {
        if let Ok(paddr) = arch.virt_to_phys(mem, dtb, addr) {
            mem.phys_write_raw(paddr, data)?;
        }
    } else {
        let mut base = addr;

        let (mut start_buf, mut end_buf) =
            data.split_at(std::cmp::min(aligned_len.as_usize(), data.len()));

        for i in [start_buf, end_buf].iter_mut() {
            for chunk in i.chunks(page_size.as_usize()) {
                if let Ok(paddr) = arch.virt_to_phys(mem, dtb, base) {
                    mem.phys_write_raw(paddr, chunk)?;
                }
                base += Length::from(chunk.len());
            }
        }
    }

    Ok(())
}

#[allow(unused)]
pub fn virt_page_info<T: AccessPhysicalMemory>(
    mem: &mut T,
    arch: Architecture,
    dtb: Address,
    addr: Address,
) -> Result<Page> {
    let paddr = arch.virt_to_phys(mem, dtb, addr)?;
    Ok(paddr
        .page
        .ok_or_else(|| Error::new("page info not found"))?)
}
