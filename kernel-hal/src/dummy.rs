use super::*;
use crate::vdso::VdsoConstants;
use acpi::Acpi;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::future::Future;
use core::ops::FnOnce;
use core::pin::Pin;
use core::time::Duration;

type ThreadId = usize;

/// The error type which is returned from HAL functions.
#[derive(Debug)]
pub struct HalError;

/// The result type returned by HAL functions.
pub type Result<T> = core::result::Result<T, HalError>;

#[repr(C)]
pub struct Thread {
    id: ThreadId,
}

impl Thread {
    /// Spawn a new thread.
    #[linkage = "weak"]
    #[export_name = "hal_thread_spawn"]
    pub fn spawn(
        _future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        _vmtoken: usize,
    ) -> Self {
        unimplemented!()
    }

    /// Set tid and pid of current task.
    #[linkage = "weak"]
    #[export_name = "hal_thread_set_tid"]
    pub fn set_tid(_tid: u64, _pid: u64) {
        unimplemented!()
    }

    /// Get tid and pid of current task.
    #[linkage = "weak"]
    #[export_name = "hal_thread_get_tid"]
    pub fn get_tid() -> (u64, u64) {
        unimplemented!()
    }
}

#[linkage = "weak"]
#[export_name = "hal_context_run"]
pub fn context_run(_context: &mut UserContext) {
    unimplemented!()
}

pub trait PageTableTrait: Sync + Send {
    /// Map the page of `vaddr` to the frame of `paddr` with `flags`.
    fn map(&mut self, _vaddr: VirtAddr, _paddr: PhysAddr, _flags: MMUFlags) -> Result<()>;

    /// Unmap the page of `vaddr`.
    fn unmap(&mut self, _vaddr: VirtAddr) -> Result<()>;

    /// Change the `flags` of the page of `vaddr`.
    fn protect(&mut self, _vaddr: VirtAddr, _flags: MMUFlags) -> Result<()>;

    /// Query the physical address which the page of `vaddr` maps to.
    fn query(&mut self, _vaddr: VirtAddr) -> Result<PhysAddr>;

    /// Get the physical address of root page table.
    fn table_phys(&self) -> PhysAddr;

    #[cfg(target_arch = "riscv64")]
    /// Activate this page table
    fn activate(&self);

    fn map_many(
        &mut self,
        mut vaddr: VirtAddr,
        paddrs: &[PhysAddr],
        flags: MMUFlags,
    ) -> Result<()> {
        for &paddr in paddrs {
            self.map(vaddr, paddr, flags)?;
            vaddr += PAGE_SIZE;
        }
        Ok(())
    }

    fn map_cont(
        &mut self,
        mut vaddr: VirtAddr,
        paddr: PhysAddr,
        pages: usize,
        flags: MMUFlags,
    ) -> Result<()> {
        for i in 0..pages {
            let paddr = paddr + i * PAGE_SIZE;
            self.map(vaddr, paddr, flags)?;
            vaddr += PAGE_SIZE;
        }
        Ok(())
    }

    fn unmap_cont(&mut self, vaddr: VirtAddr, pages: usize) -> Result<()> {
        for i in 0..pages {
            self.unmap(vaddr + i * PAGE_SIZE)?;
        }
        Ok(())
    }
}

/// Page Table
#[repr(C)]
pub struct PageTable {
    table_phys: PhysAddr,
}

impl PageTable {
    /// Get current page table
    #[linkage = "weak"]
    #[export_name = "hal_pt_current"]
    pub fn current() -> Self {
        unimplemented!()
    }

    /// Create a new `PageTable`.
    #[allow(clippy::new_without_default)]
    #[linkage = "weak"]
    #[export_name = "hal_pt_new"]
    pub fn new() -> Self {
        unimplemented!()
    }
}

impl PageTableTrait for PageTable {
    /// Map the page of `vaddr` to the frame of `paddr` with `flags`.
    #[linkage = "weak"]
    #[export_name = "hal_pt_map"]
    fn map(&mut self, _vaddr: VirtAddr, _paddr: PhysAddr, _flags: MMUFlags) -> Result<()> {
        unimplemented!()
    }
    /// Unmap the page of `vaddr`.
    #[linkage = "weak"]
    #[export_name = "hal_pt_unmap"]
    fn unmap(&mut self, _vaddr: VirtAddr) -> Result<()> {
        unimplemented!()
    }
    /// Change the `flags` of the page of `vaddr`.
    #[linkage = "weak"]
    #[export_name = "hal_pt_protect"]
    fn protect(&mut self, _vaddr: VirtAddr, _flags: MMUFlags) -> Result<()> {
        unimplemented!()
    }
    /// Query the physical address which the page of `vaddr` maps to.
    #[linkage = "weak"]
    #[export_name = "hal_pt_query"]
    fn query(&mut self, _vaddr: VirtAddr) -> Result<PhysAddr> {
        unimplemented!()
    }
    /// Get the physical address of root page table.
    #[linkage = "weak"]
    #[export_name = "hal_pt_table_phys"]
    fn table_phys(&self) -> PhysAddr {
        self.table_phys
    }

    /// Activate this page table
    #[cfg(target_arch = "riscv64")]
    #[linkage = "weak"]
    #[export_name = "hal_pt_activate"]
    fn activate(&self) {
        unimplemented!()
    }

    #[linkage = "weak"]
    #[export_name = "hal_pt_unmap_cont"]
    fn unmap_cont(&mut self, vaddr: VirtAddr, pages: usize) -> Result<()> {
        for i in 0..pages {
            self.unmap(vaddr + i * PAGE_SIZE)?;
        }
        Ok(())
    }
}

#[repr(C)]
pub struct PhysFrame {
    paddr: PhysAddr,
}

impl PhysFrame {
    #[linkage = "weak"]
    #[export_name = "hal_frame_alloc"]
    pub fn alloc() -> Option<Self> {
        unimplemented!()
    }

    #[linkage = "weak"]
    #[export_name = "hal_frame_alloc_contiguous"]
    pub fn alloc_contiguous_base(_size: usize, _align_log2: usize) -> Option<PhysAddr> {
        unimplemented!()
    }

    pub fn alloc_contiguous(size: usize, align_log2: usize) -> Vec<Self> {
        PhysFrame::alloc_contiguous_base(size, align_log2).map_or(Vec::new(), |base| {
            (0..size)
                .map(|i| PhysFrame {
                    paddr: base + i * PAGE_SIZE,
                })
                .collect()
        })
    }

    pub fn addr(&self) -> PhysAddr {
        self.paddr
    }

    #[linkage = "weak"]
    #[export_name = "hal_zero_frame_paddr"]
    pub fn zero_frame_addr() -> PhysAddr {
        unimplemented!()
    }
}

impl Drop for PhysFrame {
    #[linkage = "weak"]
    #[export_name = "hal_frame_dealloc"]
    fn drop(&mut self) {
        unimplemented!()
    }
}

/// Read physical memory from `paddr` to `buf`.
#[linkage = "weak"]
#[export_name = "hal_pmem_read"]
pub fn pmem_read(_paddr: PhysAddr, _buf: &mut [u8]) {
    unimplemented!()
}

/// Write physical memory to `paddr` from `buf`.
#[linkage = "weak"]
#[export_name = "hal_pmem_write"]
pub fn pmem_write(_paddr: PhysAddr, _buf: &[u8]) {
    unimplemented!()
}

/// Zero physical memory at `[paddr, paddr + len)`
#[linkage = "weak"]
#[export_name = "hal_pmem_zero"]
pub fn pmem_zero(_paddr: PhysAddr, _len: usize) {
    unimplemented!()
}

/// Copy content of `src` frame to `target` frame.
#[linkage = "weak"]
#[export_name = "hal_frame_copy"]
pub fn frame_copy(_src: PhysAddr, _target: PhysAddr) {
    unimplemented!()
}

/// Flush the physical frame.
#[linkage = "weak"]
#[export_name = "hal_frame_flush"]
pub fn frame_flush(_target: PhysAddr) {
    unimplemented!()
}

/// Register a callback of serial readable event.
#[linkage = "weak"]
#[export_name = "hal_serial_set_callback"]
pub fn serial_set_callback(_callback: Box<dyn Fn() -> bool + Send + Sync>) {
    unimplemented!()
}

/// Read a string from console.
#[linkage = "weak"]
#[export_name = "hal_serial_read"]
pub fn serial_read(_buf: &mut [u8]) -> usize {
    unimplemented!()
}

/// Output a string to console.
#[linkage = "weak"]
#[export_name = "hal_serial_write"]
pub fn serial_write(_s: &str) {
    unimplemented!()
}

/// Get current time.
#[linkage = "weak"]
#[export_name = "hal_timer_now"]
pub fn timer_now() -> Duration {
    unimplemented!()
}

/// Set a new timer. After `deadline`, the `callback` will be called.
#[linkage = "weak"]
#[export_name = "hal_timer_set"]
pub fn timer_set(_deadline: Duration, _callback: Box<dyn FnOnce(Duration) + Send + Sync>) {
    unimplemented!()
}

#[linkage = "weak"]
#[export_name = "hal_timer_set_next"]
pub fn timer_set_next() {
    unimplemented!()
}

/// Check timers, call when timer interrupt happened.
#[linkage = "weak"]
#[export_name = "hal_timer_tick"]
pub fn timer_tick() {
    unimplemented!()
}

pub struct InterruptManager {}
impl InterruptManager {
    /// Enable IRQ.
    #[linkage = "weak"]
    #[export_name = "hal_irq_enable"]
    pub fn enable_irq(_vector: u32) {
        unimplemented!()
    }
    /// Disable IRQ.
    #[linkage = "weak"]
    #[export_name = "hal_irq_disable"]
    pub fn disable_irq(_vector: u32) {
        unimplemented!()
    }
    /// Is a valid IRQ number.
    #[linkage = "weak"]
    #[export_name = "hal_irq_isvalid"]
    pub fn is_valid_irq(_vector: u32) -> bool {
        unimplemented!()
    }

    /// Configure the specified interrupt vector.  If it is invoked, it muust be
    /// invoked prior to interrupt registration.
    #[linkage = "weak"]
    #[export_name = "hal_irq_configure"]
    pub fn configure_irq(_vector: u32, _trig_mode: bool, _polarity: bool) -> bool {
        unimplemented!()
    }
    /// Add an interrupt handle to an IRQ
    #[linkage = "weak"]
    #[export_name = "hal_irq_register_handler"]
    pub fn register_irq_handler(_vector: u32, _handle: Box<dyn Fn() + Send + Sync>) -> Option<u32> {
        unimplemented!()
    }
    /// Remove the interrupt handle to an IRQ
    #[linkage = "weak"]
    #[export_name = "hal_irq_unregister_handler"]
    pub fn unregister_irq_handler(_vector: u32) -> bool {
        unimplemented!()
    }
    /// Handle IRQ.
    #[linkage = "weak"]
    #[export_name = "hal_irq_handle"]
    pub fn handle_irq(_vector: u32) {
        unimplemented!()
    }

    /// Method used for platform allocation of blocks of MSI and MSI-X compatible
    /// IRQ targets.
    #[linkage = "weak"]
    #[export_name = "hal_msi_allocate_block"]
    pub fn msi_allocate_block(_irq_num: u32) -> Option<(usize, usize)> {
        unimplemented!()
    }
    /// Method used to free a block of MSI IRQs previously allocated by msi_alloc_block().
    /// This does not unregister IRQ handlers.
    #[linkage = "weak"]
    #[export_name = "hal_msi_free_block"]
    pub fn msi_free_block(_irq_start: u32, _irq_num: u32) {
        unimplemented!()
    }
    /// Register a handler function for a given msi_id within an msi_block_t. Passing a
    /// NULL handler will effectively unregister a handler for a given msi_id within the
    /// block.
    #[linkage = "weak"]
    #[export_name = "hal_msi_register_handler"]
    pub fn msi_register_handler(
        _irq_start: u32,
        _irq_num: u32,
        _msi_id: u32,
        _handle: Box<dyn Fn() + Send + Sync>,
    ) {
        unimplemented!()
    }
}

/// Get platform specific information.
#[linkage = "weak"]
#[export_name = "hal_vdso_constants"]
pub fn vdso_constants() -> VdsoConstants {
    unimplemented!()
}

/// Get fault address of the last page fault.
#[linkage = "weak"]
#[export_name = "fetch_fault_vaddr"]
pub fn fetch_fault_vaddr() -> VirtAddr {
    unimplemented!()
}

#[linkage = "weak"]
#[export_name = "fetch_trap_num"]
pub fn fetch_trap_num(_context: &UserContext) -> usize {
    unimplemented!()
}

/// Get physical address of `acpi_rsdp` and `smbios` on x86_64.
#[linkage = "weak"]
#[export_name = "hal_pc_firmware_tables"]
pub fn pc_firmware_tables() -> (u64, u64) {
    unimplemented!()
}

/// Get ACPI Table
#[linkage = "weak"]
#[export_name = "hal_acpi_table"]
pub fn get_acpi_table() -> Option<Acpi> {
    unimplemented!()
}

/// IO Ports access on x86 platform
#[linkage = "weak"]
#[export_name = "hal_outpd"]
pub fn outpd(_port: u16, _value: u32) {
    unimplemented!()
}

#[linkage = "weak"]
#[export_name = "hal_inpd"]
pub fn inpd(_port: u16) -> u32 {
    unimplemented!()
}

/// Get local APIC ID
#[linkage = "weak"]
#[export_name = "hal_apic_local_id"]
pub fn apic_local_id() -> u8 {
    unimplemented!()
}

/// Fill random bytes to the buffer
#[cfg(target_arch = "x86_64")]
pub fn fill_random(buf: &mut [u8]) {
    // TODO: optimize
    for x in buf.iter_mut() {
        let mut r = 0;
        unsafe {
            core::arch::x86_64::_rdrand16_step(&mut r);
        }
        *x = r as _;
    }
}

#[cfg(target_arch = "aarch64")]
pub fn fill_random(_buf: &mut [u8]) {
    // TODO
}

#[cfg(target_arch = "riscv64")]
pub fn fill_random(_buf: &mut [u8]) {
    // TODO
}

#[linkage = "weak"]
#[export_name = "hal_rand"]
pub fn rand() -> u64 {
    unimplemented!()
}

#[linkage = "weak"]
#[export_name = "hal_current_pgtable"]
pub fn current_page_table() -> usize {
    unimplemented!()
}

#[linkage = "weak"]
#[export_name = "hal_mice_set_callback"]
pub fn mice_set_callback(_callback: Box<dyn Fn([u8; 3]) + Send + Sync>) {
    unimplemented!()
}

#[linkage = "weak"]
#[export_name = "hal_kbd_set_callback"]
pub fn kbd_set_callback(_callback: Box<dyn Fn(u16, i32) + Send + Sync>) {
    unimplemented!()
}
