use crate::device::acpi::cedt::{
    CEDT, CEDTStructureType, CXLFixedMemoryWindowStructure, CXLHostBridgeStructure,
};
use crate::device::pci::PciBus;
use crate::memory::vma::VmaType;
use crate::memory::{MemorySpace, PAGE_SIZE};
use crate::{acpi_tables, efi_services_available, pci_bus, process_manager};
use acpi::AcpiTable;
use alloc::borrow::ToOwned;
use alloc::vec::Vec;
use bitfield_struct::bitfield;
use uefi::runtime::Time;
use core::ptr;
use log::info;
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

pub fn init() {
    // as a demo for cl support we take a closer look to the cxl host bridge component registers
    if let Ok(cedt) = acpi_tables().lock().find_table::<CEDT>() {
        if let Some(range) = cedt.get_host_bridge_structures().first() {
            let data_ptr = range.as_phys_frame_range().start.start_address().as_u64() as *mut u8; //20456  das ist die höchste Addr die geht
            info!("datapointer ist {:?}", data_ptr);

            unsafe {
                // hier werden alle capabilities gescannt:
                let cxl_capability_header = data_ptr
                    .offset(CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET as isize)
                    as *mut CXLCapabilityHeader;
                info!(
                    "general capability header: {:?} liegt an adresse {:?}",
                    cxl_capability_header.read(),
                    cxl_capability_header
                );
                let end = cxl_capability_header.read().get_len();
                for i in 1..end {
                    let current_capability = data_ptr
                        .offset((CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 4 * i) as isize)
                        as *mut GeneralCXLCapabilityHeader;
                    let read_capability = current_capability.read();
                    info!("found capability: {:?}", read_capability);

                    // gerade wird die hdm decoder capability structure benoetigt
                    if read_capability.get_type() == CXLHDMDECODER_CAPABILITY {
                        // springe nun zu dem Pointer, der Capability

                        //achtung: immer vom data pointer ausgehen, denn .offset (a + b ) != .offset(a).offset(b)
                        let offset_to_capability = read_capability.get_pointer();
                        let hdm_addr = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET) as isize,
                        )
                            as *mut CXLHDMDecoderCapabilityRegister;
                        let hdm_decoder_global_cr = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 4)
                                as isize,
                        )
                            as *mut CXLHDMDecoderGlobalControlRegister;
                        info!("found decoder capability: {:?}", hdm_addr.read());
                        info!(
                            "found global decoder capability: {:?}",
                            hdm_decoder_global_cr.read()
                        );

                        let cxl_hdm_decoder0_base_low_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 16)
                                as isize,
                        )
                            as *mut u32;
                        let cxl_hdm_decoder0_base_high_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 20)
                                as isize,
                        )
                            as *mut u32;
                        let cxl_hdm_decoder0_size_low_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 24)
                                as isize,
                        )
                            as *mut u32;
                        let cxl_hdm_decoder0_size_high_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 28)
                                as isize,
                        )
                            as *mut u32;
                        let cxl_hdm_decoder0_control_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 32)
                                as isize,
                        )
                            as *mut u32;
                        let cxl_hdm_decoder0_target_list_low_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 36)
                                as isize,
                        )
                            as *mut u32;
                        let cxl_hdm_decoder0_target_list_high_register = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 40)
                                as isize,
                        )
                            as *mut u32;
                        let reserved = data_ptr.offset(
                            (offset_to_capability + CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET + 44)
                                as isize,
                        ) as *mut u32;
                        info!("decoder0 {:?}", cxl_hdm_decoder0_base_low_register.read());
                        info!("decoder0 {:?}", cxl_hdm_decoder0_base_high_register.read());
                        info!("decoder0 {:?}", cxl_hdm_decoder0_size_low_register.read());
                        info!("decoder0 {:?}", cxl_hdm_decoder0_size_high_register.read());
                        info!("decoder0 {:?}", cxl_hdm_decoder0_control_register.read());
                        info!(
                            "decoder0 {:?}",
                            cxl_hdm_decoder0_target_list_low_register.read()
                        );
                        info!(
                            "decoder0 {:?}",
                            cxl_hdm_decoder0_target_list_high_register.read()
                        );
                    }
                }

                // hier wird das erste register an dem offset der cxl arb mux aus den component registern gelesen
                let tm_control = data_ptr.offset(CXL_ARB_MUX_REGISTER_OFFSET as isize) as *mut u32; // das Register hat die groesse u32 und muss vollständig gelesen werden
                info!("Timeout control: {:x}", tm_control.read());

                let error_status =
                    data_ptr.offset((CXL_ARB_MUX_REGISTER_OFFSET + 4) as isize) as *mut u32;
                info!("error status: {:x}", error_status.read());
                let error_mask =
                    data_ptr.offset((CXL_ARB_MUX_REGISTER_OFFSET + 8) as isize) as *mut u32;
                info!("error mask: {:x}", error_mask.read());
            }

            // Read last boot time from NVRAM
            let data = unsafe { data_ptr.offset(CXL_ARB_MUX_REGISTER_OFFSET as isize).read() }; // Hier ist das Problem
            // auf das array kann nicht zugegriffen werden
            //info!("found data is: {:?}", data.get_cxlcachemem_primary_range());
        }
    }
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

pub fn demo() {
    if let Ok(cedt) = acpi_tables().lock().find_table::<CEDT>() {
        //init srat
        //srat::init();
        info!("test");

        info!("Found CEDT table");
        let structures = cedt.get_structures();
        for structure in structures {
            if structure.typ == CEDTStructureType::CXLHostBridgeStructure {
                let current: &CXLHostBridgeStructure = structure.as_structure();
                info!("Host Bridge ist {:?}", current);
                info!("Host Bridge hat die folgenden Root Ports:");
                PciBus::scan_by_nr(current.uid as u8);
                /*let base = current.base;
                let regs = base as *const[u8;40];
                unsafe{
                    let array:[u8;40] = ptr::read(regs);
                }
                info!("current.base ist {:?} und regs ist {:?}", base, regs);
                */

                //erste Addr 7247757312
                //Länge je 65536

                //zweite Addr 7247822848

                // zwischen den Adressen finden sich exakt die control register. leider komme ich noch nicht dran

                /*unsafe {
                    let help_ptr: *const CXLHostBridgeComponentRegisterRanges = current.base as *const CXLHostBridgeComponentRegisterRanges;
                    let current_ctrl_registers: CXLHostBridgeComponentRegisterRanges = *help_ptr;
                    info!("Die ctrl Register sind {:?}", current_ctrl_registers);
                }*/
            } else if structure.typ == CEDTStructureType::CXLFixedMemoryWindowStructure {
                let current: &CXLFixedMemoryWindowStructure = structure.as_structure();
                info!("Memory Window ist ist {:?}", current);
            } else {
                info!("found different structure");
            }
        }

        // Search CEDT table for non-volatile memory ranges
        for spa in cedt.get_host_bridge_structures() {
            // Copy values to avoid unaligned access of packed struct fields
            let address: u64 = spa.base;
            let length: u64 = spa.length;
            info!(
                "Found host bridge memory from cedt1 (Address: [0x{:x}], Length: [{} KB])",
                address,
                length / 1024
            );
            info!(
                "mapping von length/PAGE_Size ist {:?}",
                length / PAGE_SIZE as u64
            ); // da wir 4kb Pages haben, werden 16 Pages alloziiert

            // Map non-volatile memory range to kernel address space
            let start_page = Page::from_start_address(VirtAddr::new(address)).unwrap();
            info!(
                "page range ist {:?}",
                PageRange {
                    start: start_page,
                    end: start_page + (length / PAGE_SIZE as u64)
                }
            );
            process_manager()
                .read()
                .kernel_process()
                .expect("Failed to get kernel process")
                .virtual_address_space
                .alloc_vma(
                    start_page,
                    MemorySpace::Kernel,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    VmaType::DeviceMemory,
                    "cxlhb",
                );

            // per host bridge there is a control register space. this is the address space that was mapped before
            //now some bits are beeing set
        }

        /*for spa in cedt.get_mem_win_structures() {
            // Copy values to avoid unaligned access of packed struct fields
            let address:u64 = spa.base_hpa;
            let length:u64 = spa.window_size;
            info!("Found memory window structure from cedt (Address: [0x{:x}], Length: [{} KB])", address, length/1024);
            info!("mapping von length/PAGE_Size ist {:?}", length / PAGE_SIZE as u64); // da wir 4kb Pages haben, werden 16 Pages alloziiert

            // Map non-volatile memory range to kernel address space
            let start_page = Page::from_start_address(VirtAddr::new(address)).unwrap();
            info!("page range ist {:?}", PageRange { start: start_page, end: start_page + (length / PAGE_SIZE as u64)});
            process_manager().read().kernel_process().expect("Failed to get kernel process")
                .virtual_address_space
                .map(
                    PageRange {
                        start: start_page,
                        end: start_page + (length / PAGE_SIZE as u64),
                    },
                    MemorySpace::Kernel,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    VmaType::DeviceMemory,
                    "cxlmw",
                );

            // per host bridge there is a control register space. this is the address space that was mapped before
            //now some bits are beeing set

        }*/

        /*
        // hier wird eine hardcoded adresse eingemappt
        let hardcoded_add: u64 = 0x81800000;
        let hardcoded_len: u64 = 2097151;
        info!("Found non-volatile memory from cedt2 (Address: [0x{:x}], Length: [{} MiB])", hardcoded_add, hardcoded_len / 1024 / 1024);
        let start_page = Page::from_start_address(VirtAddr::new(hardcoded_add)).unwrap();
        process_manager().read().kernel_process().expect("Failed to get kernel process")
            .address_space()
            .map(PageRange { start: start_page, end: start_page + (hardcoded_len / PAGE_SIZE as u64) }, MemorySpace::Kernel, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        */
    }
}
