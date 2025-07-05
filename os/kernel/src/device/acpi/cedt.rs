use core::ptr;

use acpi::{sdt::{SdtHeader, Signature}, AcpiTable};
use alloc::vec::Vec;
use log::info;
use uefi::boot::PAGE_SIZE;
use x86_64::{structures::paging::{frame::PhysFrameRange, PhysFrame}, PhysAddr};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CEDT {
    header: SdtHeader,
}


#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CEDTStructureType {
    CXLHostBridgeStructure = 0,
    CXLFixedMemoryWindowStructure = 1,
    CXLXORInterleaveMathStructure = 2,
    RCECDownstreamPortAssociationStructure = 3,
    CXLSystemDescriptionStructure = 4,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CEDTStructureHeader {
    pub typ: CEDTStructureType,
    reserved_1: u8,
    pub record_length: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLHostBridgeStructure{
    pub header: CEDTStructureHeader,
    pub uid: u32,
    pub cxl_version: u32,
    reserved_2: u32,
    pub base: u64,
    pub length: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLFixedMemoryWindowStructure{
    pub header: CEDTStructureHeader,
    reserved_2: u32,
    pub base_hpa: u64,
    pub window_size: u64,
    pub encoded_nr_of_interleave_ways: u8,
    pub interleave_arithmetic: u8,
    reserved_3: u16,
    pub host_bridge_interleave_granularity: u64,
    pub window_restrictions: u16,
    pub qtg_id: u16,
    pub interleave_target_list: [u32; 2], //hier ist die groesse 4* Anzahl encodet interleave ways
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLXORInterleaveMathStructure{
    pub header: CEDTStructureHeader,
    reserved_2: u16,
    pub nr_of_bitmap_entries: u8,
    pub xormap_list: u128, // hier muss 8*Anzahl vor nr_of_bitmap_entries
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RCECDownstreamPortAssociationStructure{
    pub header: CEDTStructureHeader,
    pub rcec_segment_nr: u16,
    pub rcec_bdf: u16,
    pub protocol_type: u16,
    pub base_addr: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CXLSystemDescriptionStructure{
    pub header: CEDTStructureHeader,
    pub system_capabilities: u16,
    reserved_2: u16,
}

unsafe impl AcpiTable for CEDT {
    const SIGNATURE: Signature = Signature::CEDT;

    fn header(&self) -> &SdtHeader {
        &self.header
    }
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

