use alloc::borrow::ToOwned;
use alloc::vec::Vec;
use core::ptr;
use acpi::{AcpiTable};
use acpi::sdt::{SdtHeader, Signature};
use bitfield_struct::bitfield;
use log::info;
use crate::memory::{MemorySpace, PAGE_SIZE};
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame};
use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::page::PageRange;
use x86_64::{PhysAddr, VirtAddr};
use crate::{acpi_tables, pci_bus, process_manager};
use crate::device::pci::PciBus;
use crate::memory::srat::MemoryAffinityStructure;
use crate::memory::vmm::VmaType;

pub fn print_bus_devices(){
    pci_bus().dump_devices();
}

pub fn print_bus_devices_status(){
    pci_bus().dump_devices_status_registers();
}

pub fn print_bus_devices_command(){
    pci_bus().dump_devices_command_registers();
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CEDT {
    header: SdtHeader,

}



#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum CEDTStructureType {
    CXLHostBridgeStructure = 0,
    CXLFixedMemoryWindowStructure = 1,
    CXLXORInterleaveMathStructure = 2,
    RCECDownstreamPortAssociationStructure = 3,
    CXLSystemDescriptionStructure = 4,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CEDTStructureHeader {
    typ: CEDTStructureType,
    reserved_1: u8,
    record_length: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLHostBridgeStructure{
    header: CEDTStructureHeader,
    uid: u32,
    cxl_version: u32,
    reserved_2: u32,
    base: u64,
    length: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLFixedMemoryWindowStructure{
    header: CEDTStructureHeader,
    reserved_2: u32,
    base_hpa: u64,
    window_size: u64,
    encoded_nr_of_interleave_ways: u8,
    interleave_arithmetic: u8,
    reserved_3: u16,
    host_bridge_interleave_granularity: u64,
    window_restrictions: u16,
    qtg_id: u16,
    interleave_target_list: [u32; 2], //hier ist die groesse 4* Anzahl encodet interleave ways
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLXORInterleaveMathStructure{
    header: CEDTStructureHeader,
    reserved_2: u16,
    nr_of_bitmap_entries: u8,
    xormap_list: u128, // hier muss 8*Anzahl vor nr_of_bitmap_entries
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RCECDownstreamPortAssociationStructure{
    header: CEDTStructureHeader,
    rcec_segment_nr: u16,
    rcec_bdf: u16,
    protocol_type: u16,
    base_addr: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLSystemDescriptionStructure{
    header: CEDTStructureHeader,
    system_capabilities: u16,
    reserved_2: u16,
}

pub const CXL_IO_REGISTER_OFFSET: usize = 0;
pub const CXL_CACHE_MEM_PRIMARY_RANGE_OFFSET: usize = 1024 * 4;
pub const IMPLEMENTATION_SPECIFIC_OFFEST: usize = 1024 * 4 + 1024 * 4;
pub const CXL_ARB_MUX_REGISTER_OFFSET: usize = 1024 * 4 + 1024 * 4 + 1024 * 48;

// enum passt wegen bitfield nicht
pub const CXLNULL_CAPABILITY:usize = 0;
pub const CXLCAPABILITY:usize = 1;
pub const CXLRAS_CAPABILITY:usize = 2;
pub const CXLSECURITY_CAPABILITY:usize = 3;
pub const CXLLINK_CAPABILITY:usize = 4;
pub const CXLHDMDECODER_CAPABILITY:usize = 5;
pub const CXLEXTENDED_SECURITY_CAPABILITY:usize = 6;
pub const CXKIDECAPABILITY:usize = 7;
pub const CXLSNOOP_FILTER_CAPABILITY:usize = 8;
pub const CXLTIMEOUTAND_ISOLATION_CAPABILITY:usize = 9;
pub const CXLCACHEMEM_EXTENDED_REGISTER_CAPABILITY:usize = 10;
pub const CXLBIROUTE_TABLE_CAPABILITY:usize = 11;
pub const CXLBIDECODER_CAPABILITY:usize = 12;
pub const CXLCACHE_IDROUTE_TABLE_CAPABILITY:usize = 13;
pub const CXLCACHE_IDDECODER_CAPABILITY:usize = 14;
pub const CXLEXTENDED_HDMDECODER_CAPABILITY:usize = 15;
pub const CXLEXTENDED_METADATA_CAPABILITY:usize = 16;



#[bitfield(u32)]
pub struct CXLCapabilityHeader{
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
pub struct GeneralCXLCapabilityHeader{
    #[bits(16)]
    cxl_capability_id: usize,

    #[bits(4)]
    cxl_capability_version: usize,

    #[bits(12)]
    cxl_capability_pointer: usize,
}

#[bitfield(u32)]
pub struct CXLHDMDecoderCapabilityRegister{
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
pub struct CXLHDMDecoderGlobalControlRegister{
    cxl_capability_id: bool,

    hdm_decoder_enable: bool,

    #[bits(30)]
    reserved: usize,
}







unsafe impl AcpiTable for CEDT {
    const SIGNATURE: Signature = Signature::CEDT;

    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

impl CXLCapabilityHeader{
    pub fn get_len(&self) -> usize{
        return self.array_size()
    }
}

impl GeneralCXLCapabilityHeader {
    pub fn get_type(&self) -> usize {
        return self.cxl_capability_id()
    }
    pub fn get_pointer(&self) -> usize{ return self.cxl_capability_pointer();}
}


impl CEDT {
    pub fn get_structures(&self) -> Vec<&CEDTStructureHeader> {
        let mut tables = Vec::<&CEDTStructureHeader>::new();

        let mut remaining = self.header.length as usize - size_of::<CEDT>();
        info!("################remaining ist {:?}", self.header);
        let mut structure_ptr = unsafe { ptr::from_ref(self).add(1) } as *const CEDTStructureHeader;

        while remaining > 0 {
            unsafe {
                let structure = *structure_ptr;
                tables.push(structure_ptr.as_ref().expect("Invalid CEDT structure"));
                info!("gefundene Structure is {:?}", structure);

                structure_ptr = (structure_ptr as *const u8).add(structure.record_length as usize) as *const CEDTStructureHeader;
                info!("remaining = {:?} und recordlen = {:?}", remaining, structure.record_length as usize);
                remaining = remaining - structure.record_length as usize;
            }
            info!("Found CEDT Structure");
        }
        info!("###+++++ das ist nach dem get_structures {:?}", tables);

        return tables;
    }

    pub fn get_host_bridge_structures (&self) -> Vec<&CXLHostBridgeStructure> {
        let mut structures = Vec::<&CXLHostBridgeStructure>::new();

        self.get_structures().iter().for_each(|structure| {
            let structure_type = unsafe { ptr::from_ref(structure).read_unaligned().typ };
            if structure_type == CEDTStructureType::CXLHostBridgeStructure {
                structures.push(structure.as_structure::<CXLHostBridgeStructure>());
            }
        });

        return structures;
    }

    pub fn get_mem_win_structures (&self) -> Vec<&CXLFixedMemoryWindowStructure> {
        let mut structures = Vec::<&CXLFixedMemoryWindowStructure>::new();

        self.get_structures().iter().for_each(|structure| {
            let structure_type = unsafe { ptr::from_ref(structure).read_unaligned().typ };
            if structure_type == CEDTStructureType::CXLFixedMemoryWindowStructure {
                structures.push(structure.as_structure::<CXLFixedMemoryWindowStructure>());
            }
        });

        return structures;
    }
}

impl CXLFixedMemoryWindowStructure{
    pub fn as_phys_frame_range(&self) -> PhysFrameRange {
        let address:u64 = self.base_hpa;
        let length:u64 = self.window_size;
        let start = PhysFrame::from_start_address(PhysAddr::new(address)).expect("Invalid start address");

        return PhysFrameRange { start, end: start + (length / PAGE_SIZE as u64) };
    }
}

impl CXLHostBridgeStructure{
    pub fn as_phys_frame_range(&self) -> PhysFrameRange {
        let address:u64 = self.base;
        let length:u64 = self.length;
        let start = PhysFrame::from_start_address(PhysAddr::new(address)).expect("Invalid start address");

        return PhysFrameRange { start, end: start + (length / PAGE_SIZE as u64) };
    }
}

impl CEDTStructureHeader {
    pub fn as_structure<T>(&self) -> &T {
        unsafe {
            ptr::from_ref(self).cast::<T>().as_ref().expect("Invalid CEDT structure")
        }
    }
}

/*impl CXLHostBridgeComponentRegisterRanges{
    pub fn get_cxlcachemem_primary_range(&self) ->CXLCapabilityHeader{
        return self.cxl_cap_header;
    }
}*/


pub fn init() {
    if let Ok(cedt) = acpi_tables().lock().find_table::<CEDT>() {
        info!("Found CEDT table");
        let structures = cedt.get_structures();
        for structure in structures{
            if structure.typ == CEDTStructureType::CXLHostBridgeStructure{
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
            }else if structure.typ == CEDTStructureType::CXLFixedMemoryWindowStructure{
                let current: &CXLFixedMemoryWindowStructure = structure.as_structure();
                info!("Memory Window ist ist {:?}", current);
            }else{
                info!("found different structure");
            }
        }

        // Search CEDT table for non-volatile memory ranges
        for spa in cedt.get_host_bridge_structures() {
            // Copy values to avoid unaligned access of packed struct fields
            let address:u64 = spa.base;
            let length:u64 = spa.length;
            info!("Found host bridge memory from cedt1 (Address: [0x{:x}], Length: [{} KB])", address, length/1024);
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