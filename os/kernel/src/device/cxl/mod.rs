pub mod capabilities;

use crate::device::acpi::cedt::{
    CEDT, CEDTStructureType, CXLFixedMemoryWindowStructure, CXLHostBridgeStructure,
};
use crate::device::cxl::capabilities::{get_capabilities, CXLCapability};
use crate::device::pci::PciBus;
use crate::memory::vma::VmaType;
use crate::memory::{MemorySpace, PAGE_SIZE};
use crate::{acpi_tables, efi_services_available, pci_bus, process_manager};
use bitfield_struct::bitfield;
use log::info;
use uefi::runtime::Time;
use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::page::PageRange;
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame};
use x86_64::{PhysAddr, VirtAddr};

pub fn print_bus_devices() {
    pci_bus().dump_devices();
}

pub fn print_bus_devices_status() {
    pci_bus().dump_devices_status_registers();
}

pub fn print_bus_devices_command() {
    pci_bus().dump_devices_command_registers();
}

pub const CXL_IO_REGISTER_OFFSET: usize = 0;
pub const CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET: usize = 1024 * 4;
pub const IMPLEMENTATION_SPECIFIC_OFFEST: usize = 1024 * 4 + 1024 * 4;
pub const CXL_ARB_MUX_REGISTER_OFFSET: usize = 1024 * 4 + 1024 * 4 + 1024 * 48;

// enum passt wegen bitfield nicht
pub const CXLNULL_CAPABILITY: usize = 0;
pub const CXLCAPABILITY: usize = 1;
pub const CXLRAS_CAPABILITY: usize = 2;
pub const CXLSECURITY_CAPABILITY: usize = 3;
pub const CXLLINK_CAPABILITY: usize = 4;
pub const CXLHDMDECODER_CAPABILITY: usize = 5;
pub const CXLEXTENDED_SECURITY_CAPABILITY: usize = 6;
pub const CXKIDECAPABILITY: usize = 7;
pub const CXLSNOOP_FILTER_CAPABILITY: usize = 8;
pub const CXLTIMEOUTAND_ISOLATION_CAPABILITY: usize = 9;
pub const CXLCACHEMEM_EXTENDED_REGISTER_CAPABILITY: usize = 10;
pub const CXLBIROUTE_TABLE_CAPABILITY: usize = 11;
pub const CXLBIDECODER_CAPABILITY: usize = 12;
pub const CXLCACHE_IDROUTE_TABLE_CAPABILITY: usize = 13;
pub const CXLCACHE_IDDECODER_CAPABILITY: usize = 14;
pub const CXLEXTENDED_HDMDECODER_CAPABILITY: usize = 15;
pub const CXLEXTENDED_METADATA_CAPABILITY: usize = 16;

#[bitfield(u32)]
pub struct CXLCapabilityHeader {
    #[bits(16)]
    cxl_capability_id: usize,

    #[bits(4)]
    cxl_capability_version: usize,

    #[bits(4)]
    cxl_cache_mem_version: usize,

    #[bits(8)]
    array_size: usize,
}

#[bitfield(u32)]
pub struct GeneralCXLCapabilityHeader {
    #[bits(16)]
    cxl_capability_id: usize,

    #[bits(4)]
    cxl_capability_version: usize,

    #[bits(12)]
    cxl_capability_pointer: usize,
}

#[bitfield(u32)]
pub struct CXLHDMDecoderCapabilityRegister {
    #[bits(4)]
    decoder_count: usize,

    #[bits(4)]
    target_count: usize,

    a11to8interleave_capable: bool,
    a14to12interleave_capable: bool,
    poison_on_decode_error_capability: bool,
    three_six_twelve_way_interleave_capable: bool,
    sixteen_way_interleave_capable: bool,
    uio_capable: bool,

    #[bits(2)]
    reserved: usize,

    #[bits(4)]
    uio_capable_decoder_count: usize,

    mem_data_nxm_capable: bool,

    #[bits(2)]
    supported_coherency_models: usize,

    #[bits(9)]
    reserved2: usize,
}

#[bitfield(u32)]
pub struct CXLHDMDecoderGlobalControlRegister {
    cxl_capability_id: bool,

    hdm_decoder_enable: bool,

    #[bits(30)]
    reserved: usize,
}

impl CXLCapabilityHeader {
    pub fn get_len(&self) -> usize {
        return self.array_size();
    }
}

impl GeneralCXLCapabilityHeader {
    pub fn get_type(&self) -> usize {
        return self.cxl_capability_id();
    }
    pub fn get_pointer(&self) -> usize {
        return self.cxl_capability_pointer();
    }
}

/*impl CXLHostBridgeComponentRegisterRanges{
    pub fn get_cxlcachemem_primary_range(&self) ->CXLCapabilityHeader{
        return self.cxl_cap_header;
    }
}*/

fn map_registers(chbs: &CXLHostBridgeStructure) -> PageRange {
    let address: u64 = chbs.base;
    let length: u64 = chbs.length;
    let page_count = length / PAGE_SIZE as u64;
    info!(
        "Found host bridge memory from cedt1 (Address: [0x{:x}], Length: [{} KB]). Mapping to {} pages",
        address,
        length / 1024,
        page_count
    );

    let start_page = Page::from_start_address(VirtAddr::new(address)).unwrap();
    let start_frame = PhysFrame::from_start_address(PhysAddr::new(address)).unwrap();

    let process = process_manager()
        .read()
        .kernel_process()
        .expect("Failed to get kernel process");

    let vma = process
        .virtual_address_space
        .alloc_vma(
            Some(start_page),
            page_count,
            MemorySpace::Kernel,
            VmaType::DeviceMemory,
            "CXL_HB",
        )
        .expect("Not possible ot create VMA for CXL Host Bridge");

    process
        .virtual_address_space
        .map_pfr_for_vma(
            &vma,
            PhysFrameRange {
                start: start_frame,
                end: start_frame + page_count,
            },
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        )
        .expect("Not possible ot vma to physical memory");

    vma.range()
}

pub fn demo_capabilities(chbs: &CXLHostBridgeStructure) {
    let register_pages = map_registers(chbs);
    let primary_range = register_pages
        .clone()
        .nth(1)
        .expect("Primary Range not contained in CHBS range");

    for capability in get_capabilities(&primary_range).expect("There should be CXL Capabilities"){
        info!("found capability: {:?}", capability);
    }

    let base_ptr = register_pages.start.start_address().as_ptr::<u8>();

    unsafe {
        // hier wird das erste register an dem offset der cxl arb mux aus den component registern gelesen
        let tm_control = base_ptr.offset(CXL_ARB_MUX_REGISTER_OFFSET as isize) as *mut u32; // das Register hat die groesse u32 und muss vollständig gelesen werden
        info!("Timeout control: {:x}", tm_control.read());

        let error_status = base_ptr.offset((CXL_ARB_MUX_REGISTER_OFFSET + 4) as isize) as *mut u32;
        info!("error status: {:x}", error_status.read());
        let error_mask = base_ptr.offset((CXL_ARB_MUX_REGISTER_OFFSET + 8) as isize) as *mut u32;
        info!("error mask: {:x}", error_mask.read());
    }

    // Read last boot time from NVRAM
    let data = unsafe { base_ptr.offset(CXL_ARB_MUX_REGISTER_OFFSET as isize).read() }; // Hier ist das Problem
    // auf das array kann nicht zugegriffen werden
    //info!("found data is: {:?}", data.get_cxlcachemem_primary_range());
}

pub fn demo_hardcoded_addr() {
    // As a demo for cxl support using a hardcoded addr we found in the system using info pci, we read the last boot time from NVRAM and write the current boot time to it
    let date_ptr3 = 0x81800000 as *mut Time;
    info!("--------- date ptr cxl_hardcoded ist {:?}", date_ptr3);
    // Read last boot time from NVRAM
    let date = unsafe { date_ptr3.read() };
    if date.is_valid().is_ok() {
        info!(
            "Last boot time hardcoced: [{:0>4}-{:0>2}-{:0>2} {:0>2}:{:0>2}:{:0>2}]",
            date.year(),
            date.month(),
            date.day(),
            date.hour(),
            date.minute(),
            date.second()
        );
    } else {
        info!("hardcoded time not found");
    }

    if efi_services_available() {
        if let Ok(time) = uefi::runtime::get_time() {
            unsafe {
                info!("current time is {:?}", time);
                date_ptr3.write(time);
                let written = date_ptr3.read();
                info!("wrote time {:?}", written);
            }
        }
    }
}

pub fn init() {
    if let Ok(cedt) = acpi_tables().lock().find_table::<CEDT>() {
        info!("Found CEDT table");
        let structures = cedt.get_structures();
        for structure in structures {
            if structure.typ == CEDTStructureType::CXLHostBridgeStructure {
                let current: &CXLHostBridgeStructure = structure.as_structure();
                info!("Host Bridge ist {:?}", current);
                demo_capabilities(current);
                info!("Host Bridge hat die folgenden Root Ports:");
                PciBus::scan_by_nr(current.uid as u8);
            } else if structure.typ == CEDTStructureType::CXLFixedMemoryWindowStructure {
                let current: &CXLFixedMemoryWindowStructure = structure.as_structure();
                info!("Memory Window ist ist {:?}", current);
            } else {
                info!("found different structure");
            }
        }

        for spa in cedt.get_host_bridge_structures() {}

        //TODO: map CXL Memory Windows
    }
}
