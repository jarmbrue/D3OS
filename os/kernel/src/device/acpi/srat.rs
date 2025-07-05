use alloc::vec::Vec;
use core::ptr;
use acpi::sdt::{SdtHeader, Signature};
use log::info;
use crate::acpi_tables;
use acpi::AcpiTable;
use x86_64::PhysAddr;
use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::PhysFrame;
use crate::memory::PAGE_SIZE;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SRAT {
    header: SdtHeader,

}
#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum SratStructureType {
    ProcessorLocalApicAffinityStructure = 0,
    MemoryAffinityStructure = 1,
    ProcessorLocalX2apicAffinityStructure = 2,
    GiccAffinityStructure = 3,
    ArchitectureSpecificAffinityStructure = 4,
    GenericInitiatorAffinityStructure = 5,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SratStructureHeader {
    typ: SratStructureType,
    typ_2: u8,
    length: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct SratFormat {
    //laut spezifikation ist da ein header, aber osdev hat diesen nicht. nur zur Info
    signature: u32,
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: u64,
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
    reserved_1: u32,
    reserved_2: u64,
    //srat_structures: muss ich noch genauer schauen, wie das läuft
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct ProcessorLocalApicAffinityStructure {
    header: SratStructureHeader,
    proximility_domain: u8,
    apic_id: u8,
    flags: u32,
    local_sapic_eid: u8,
    proximility_domain_2: [u8; 3],
    clock_domain: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct MemoryAffinityStructure {
    header: SratStructureHeader,
    proximility_domain: u32,
    reserved: u16,
    base_addr_low: u32,
    base_addr_high: u32,
    length_low: u32,
    length_high: u32,
    reserved_2: u32,
    flags: u32,
    reserved_3: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct ProcessorLocalX2apicAffinityStructure {
    header: SratStructureHeader,
    reserved: u16,
    proximity_domain: u32,
    x2apic_id: u32,
    flags: u32,
    clock_domain: u32,
    reserved_2: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct GiccAffinityStructure {
    header: SratStructureHeader,
    proximity_domain: u32,
    acpi_processor_uid: u32,
    flags: u32,
    clock_domain: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct ArchitectureSpecificAffinityStructure {
    header: SratStructureHeader,
    proximity_domain: u32,
    reserved: u16,
    its_id: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct GenericInitiatorAffinityStructure {
    header: SratStructureHeader,
    reserved: u8,
    device_handle_type: u8,
    proximity_domain: u32,
    device_handle: u128,
    flags: u32,
    reserved_2:u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct DeviceHandleAcpi {
    acpi_hid: u64,
    acpi_uid: u32,
    reserved: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
//srat steht für static resource affinity table und zeigt alle ressourcen an, die das system kennt
pub struct DeviceHandlePci {
    pci_segment: u16,
    pci_bdf_nr: u16,
    reserved: [u8; 12],
}

unsafe impl AcpiTable for SRAT {
    const SIGNATURE: Signature = Signature::CEDT;

    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

impl SRAT {
    pub fn get_structures(&self) -> Vec<&SratStructureHeader> {
        let mut tables = Vec::<&SratStructureHeader>::new();

        let mut remaining = self.header.length as usize - size_of::<SRAT>();
        let mut structure_ptr = unsafe { ptr::from_ref(self).add(1) } as *const SratStructureHeader;

        while remaining > 0 {
            unsafe {
                let structure = *structure_ptr;
                tables.push(structure_ptr.as_ref().expect("Invalid Srat structure"));
                info!("gefundene Structure is {:?}", structure);

                structure_ptr = (structure_ptr as *const u8).add(structure.length as usize) as *const SratStructureHeader;
                info!("remaining = {:?} und recordlen = {:?}", remaining, structure.length as usize);
                remaining = remaining - structure.length as usize;
            }
            info!("Found Srat Structure");
        }

        return tables;
    }


    pub fn get_memory_structures (&self) -> Vec<&MemoryAffinityStructure> {
        let mut structures = Vec::<&MemoryAffinityStructure>::new();

        self.get_structures().iter().for_each(|structure| {
            let structure_type = unsafe { ptr::from_ref(structure).read_unaligned().typ };
            if structure_type == SratStructureType::MemoryAffinityStructure {
                structures.push(structure.as_structure::<MemoryAffinityStructure>());
            }
        });

        return structures;
    }
}

impl MemoryAffinityStructure{
    pub fn as_phys_frame_range(&self) -> PhysFrameRange {
        let address:u64 = (self.base_addr_high as u64) << 32 | (self.base_addr_low as u64);
        let length:u64 = (self.length_high as u64) << 32 | (self.length_low as u64);
        let start = PhysFrame::from_start_address(PhysAddr::new(address)).expect("Invalid start address");

        return PhysFrameRange { start, end: start + (length / PAGE_SIZE as u64) };
    }
}

impl SratStructureHeader {
    pub fn as_structure<T>(&self) -> &T {
        unsafe {
            ptr::from_ref(self).cast::<T>().as_ref().expect("Invalid Srat structure")
        }
    }
}


pub fn init() {
    if let Ok(srat) = acpi_tables().lock().find_table::<SRAT>() {
        //info!("Found SRAT table");
        let structures = srat.get_structures();
        for structure in structures{
            if structure.typ == SratStructureType::ProcessorLocalApicAffinityStructure{
                let current: &ProcessorLocalApicAffinityStructure = structure.as_structure();
                //info!("ProcessorLocalApicAffinityStructure ist {:?}", current);
            }else if structure.typ == SratStructureType::MemoryAffinityStructure{
                let current: &MemoryAffinityStructure = structure.as_structure();
                //info!("MemoryAffinityStructure ist {:?}", current);
            }else if structure.typ == SratStructureType::ProcessorLocalX2apicAffinityStructure {
                let current: &ProcessorLocalX2apicAffinityStructure = structure.as_structure();
               // info!("ProcessorLocalX2apicAffinityStructure ist {:?}", current);
            }else if structure.typ == SratStructureType::GiccAffinityStructure {
                let current: &GiccAffinityStructure = structure.as_structure();
                //info!("GiccAffinityStructure ist {:?}", current);
            }else if structure.typ == SratStructureType::ArchitectureSpecificAffinityStructure {
                let current: &ArchitectureSpecificAffinityStructure = structure.as_structure();
               // info!("ArchitectureSpecificAffinityStructure ist {:?}", current);
            }else if structure.typ == SratStructureType::GenericInitiatorAffinityStructure {
                let current: &GenericInitiatorAffinityStructure = structure.as_structure();
                //info!("GenericInitiatorAffinityStructure ist {:?}", current);
            }else{
               // info!("unknown structure");
            }

        }
        // Given addr does not work properly
        /*
        // Search SRAT table for non-volatile memory ranges
        for spa in srat.get_memory_structures() {
            // Copy values to avoid unaligned access of packed struct fields
            let address:u64 = (spa.base_addr_high as u64) << 32 | (spa.base_addr_low as u64);
            let length:u64 = (spa.length_high as u64) << 32 | (spa.length_low as u64);
            info!("Found non-volatile memory from srat (Address: [0x{:x}], Length: [{} MiB])", address, length / 1024 / 1024);

            // Map non-volatile memory range to kernel address space
            let start_page = Page::from_start_address(VirtAddr::new(address)).unwrap();
            process_manager().read().kernel_process().expect("Failed to get kernel process")
                .address_space()
                .map(PageRange { start: start_page, end: start_page + (length / PAGE_SIZE as u64) }, MemorySpace::Kernel, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        }
        */
    }
}
