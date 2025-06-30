use core::ops::BitOr;

use acpi::{
    AcpiTable,
    sdt::{SdtHeader, Signature},
};
use alloc::{sync::Arc, vec::Vec};
use log::{error, info};
use pci_types::{CommandRegister, ConfigRegionAccess, EndpointHeader};
use spin::RwLock;
use x86_64::{
    registers::debug::DebugAddressRegister,
    structures::paging::{PageTableFlags, frame::PhysFrameRange},
};

use crate::{
    acpi_tables, device::{cxl, pci::PciBus}, memory::{frames, pages, vma::VmaType, MemorySpace, PAGE_SIZE}, pci_bus, process::process::Process, process_manager
};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Cedt {
    header: SdtHeader,
}

unsafe impl AcpiTable for Cedt {
    const SIGNATURE: Signature = Signature::CEDT;

    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

impl Cedt {
    fn get_structure_headers(&self) -> Vec<&CedtStructureType> {
        let mut length = self.header.length as usize;
        length -= size_of::<SdtHeader>();

        let mut result = Vec::new();
        let mut structure_ptr =
            unsafe { core::ptr::from_ref(self).add(1) as *const CedtStructureType };
        while length > 0 {
            let structure = unsafe { &*structure_ptr };
            let l = structure.record_length();

            if !structure.is_valid() {
                error!("Structure is not valid {:?}", structure);
                break;
            }

            if length < l {
                error!("Structure is too long");
                break;
            }

            result.push(structure);

            length -= l;
            structure_ptr = unsafe { structure_ptr.byte_add(l) }
        }
        return result;
    }
}

/*
* CXL Host Bridge Structure (CHBS)
* CXL Fixed Memory Window Structure (CFMWS)
* CXL XOR Interleave Math Structure (CXIMS)
* RCEC Downstream Port Association Structure (RDPAS)
* CXL System Description Structure (CSDS)
*/
#[repr(u8)]
#[derive(Debug)]
pub enum CedtStructureType {
    CHBS(CXLHostBridgeStructure) = 0,
    CFMWS(CXLFixedMemoryWindowStructure) = 1,
    CXIMS(CXLXorInterleaveMathStructure) = 2,
    RDPAS(RCECDownstreamPortAssociationStructure) = 3,
    CSDS(CXLSystemDescriptionStructure) = 4,
}

impl CedtStructureType {
    fn record_length(&self) -> usize {
        match &self {
            Self::CHBS(s) => s.record_length.into(),
            Self::CFMWS(s) => s.record_length.into(),
            Self::CXIMS(s) => s.record_length.into(),
            Self::RDPAS(s) => s.record_length.into(),
            Self::CSDS(s) => s.record_length.into(),
        }
    }

    fn is_valid(&self) -> bool {
        match &self {
            Self::CHBS(s) => s.is_valid(),
            Self::CFMWS(s) => s.is_valid(),
            Self::CXIMS(s) => todo!(),
            Self::RDPAS(s) => todo!(),
            Self::CSDS(s) => todo!(),
        }
    }
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct CXLHostBridgeStructure {
    _reserved_1: u8,
    record_length: u16,
    uid: u32,
    cxl_version: u32,
    _reserved_2: u32,
    base: u64,
    length: u64,
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct CXLFixedMemoryWindowStructure {
    _reserved_1: u8,
    record_length: u16,
    _reserved: u32,
    base_hpa: u64,
    window_size: u64,
    eniw: u8, // Encoded Number of Interleave Ways
    interleave_arithmetic: u8,
    _reserved_2: u16,
    hbig: u32, // Host Bridge Interleave Granularity
    window_restrictions: u16,
    qtg_id: u16, // QoS Throttling Group ID
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct CXLXorInterleaveMathStructure {
    _reserved_1: u8,
    record_length: u16,
    // TODO: add fields
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct RCECDownstreamPortAssociationStructure {
    _reserved_1: u8,
    record_length: u16,
    // TODO: add fields
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct CXLSystemDescriptionStructure {
    _reserved_1: u8,
    record_length: u16,
    // TODO: add fields
}

impl CXLHostBridgeStructure {
    fn is_valid(&self) -> bool {
        self.record_length == 0x20
    }
}

impl CXLFixedMemoryWindowStructure {
    fn niw(&self) -> u32 {
        if self.eniw < 8 {
            2_u32.pow(self.eniw as u32)
        } else {
            3 * 2_u32.pow(self.eniw as u32 - 8)
        }
    }

    fn interleave_targets(&self) -> &[u32] {
        unsafe {
            alloc::slice::from_raw_parts(
                core::ptr::from_ref(self).add(1) as *const u32,
                self.niw() as usize,
            )
        }
    }

    fn is_valid(&self) -> bool {
        self.record_length as u32 == 0x24 + 4 * self.niw()
            && self.base_hpa % 256 * 1024 * 1024 == 0
            && self.window_size % 256 * 1024 * 1024 == 0
    }
}

fn map_memory(start: u64, start_physical: u64, length: u64) {
    let start_page = pages::page_from_u64(start).expect("CXL address is not page aligned");
    let start_page_frame = frames::frame_from_u64(start_physical).expect("CXL address is not page aligned");

    let kernel_process: Arc<Process> = process_manager().read().kernel_process().unwrap();

    let vma = kernel_process
        .virtual_address_space
        .alloc_vma(
            Some(start_page),
            length.div_ceil(PAGE_SIZE as u64) ,
            MemorySpace::Kernel,
            VmaType::DeviceMemory,
            "CXL",
        )
        .expect("alloc_vma failed for NVRAM");

    let phys_mem = PhysFrameRange {
        start: start_page_frame,
        end: start_page_frame + (length / PAGE_SIZE as u64),
    };

    info!("Mapping {:?} to {:?}", phys_mem, vma);

    kernel_process
        .virtual_address_space
        .map_pfr_for_vma(
            &vma,
            phys_mem,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        )
        .expect("Could not map base_address of CXL ACPI table");
}

#[derive(Debug)]
struct RCHDownstreamPortRCRB {
    null_extended_capability: u16,
    version: u8,
    next_capability_offset: u16,
    command: u16,
    status: u16,
    revision_id: u8,
    class_code: u32,
    cache_line_size: u8,
    header_type: u8,
    bar0: u64,
}

impl RCHDownstreamPortRCRB {
    unsafe fn from_base(base_address: *const u8) -> RCHDownstreamPortRCRB {
        info!("Checking CHBCR");
        // reading mmio only works in chunks of u32
        let base = unsafe { base_address.offset(0x1000) as *const u32 };
        let reg0 = unsafe { base.offset(0).read() };
        let reg1 = unsafe { base.offset(1).read() };
        let reg2 = unsafe { base.offset(2).read() };
        let reg3 = unsafe { base.offset(3).read() };
        let reg4 = unsafe { base.offset(4).read() };
        let reg5 = unsafe { base.offset(5).read() };

        RCHDownstreamPortRCRB {
            null_extended_capability: (reg0 & 0xFFFF) as u16,
            version: (reg0 >> 16 & 0xF) as u8,
            next_capability_offset: (reg0 >> 20) as u16,
            command: (reg1 & 0xFFFF) as u16,
            status: (reg1 >> 16) as u16,
            revision_id: (reg2 & 0xFF) as u8,
            class_code: reg2 >> 8,
            cache_line_size: (reg3 & 0xFF) as u8,
            header_type: (reg3 >> 16 & 0xFF) as u8,
            bar0: ((reg5 as u64) << 32) | (reg4 as u64),
        }
    }
}

fn read_chbs_base(base: u64, length: u64) {
    let base_ptr = base as *const u32;
    for i in 0..length/4 {
        let v = unsafe { base_ptr.offset(i as isize).read() };
        if v != 0 {
            info!("{:04x}: {:08x}", i*4, v);
        }
    }
}

pub fn test() {
    if let Ok(cedt) = acpi_tables().lock().find_table::<Cedt>() {
        info!("Found CEDT found");

        let structures = cedt.get_structure_headers();
        info!("Numer of CEDT Structures: {}", structures.len());

        let mut cxl_bus: Option<PciBus> = None;

        for s in structures {
            info!("{:?}", s);
            if let CedtStructureType::CHBS(chbs) = s {
                cxl_bus = Some(PciBus::scan(chbs.uid as u8));

                let base = chbs.base;
                let length = chbs.length;
                map_memory(base, base, length);
                read_chbs_base(base, length);
                let regs = unsafe { RCHDownstreamPortRCRB::from_base(base as *const u8) };
                let bar0_virt = 0x20000000;
                info!("BAR0: {:016x}", regs.bar0);
                map_memory(bar0_virt, regs.bar0, 1);
            }
        }

        if let Some(cxl_bus) = cxl_bus {
            let config_space = cxl_bus.config_space();
            for dev in &cxl_bus.devices {
                info!("{:?}", dev.read().header().revision_and_class(config_space));
            }
        }

    } else {
        error!("No CEDT table found!");
    }
}
