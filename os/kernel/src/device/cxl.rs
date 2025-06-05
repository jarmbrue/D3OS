use acpi::{
    AcpiTable,
    sdt::{SdtHeader, Signature},
};
use alloc::vec::Vec;
use core::{fmt::Debug, ops::BitOr, ptr, slice};
use log::{error, info};
use pci_types::{CommandRegister, EndpointHeader};
use spin::RwLock;

use crate::{acpi_tables, pci_bus};

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
        let mut structure_ptr = unsafe { ptr::from_ref(self).add(1) as *const CedtStructureType };
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
            slice::from_raw_parts(
                ptr::from_ref(self).add(1) as *const u32,
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

pub fn test() {
    if let Ok(cedt) = acpi_tables().lock().find_table::<Cedt>() {
        info!("Found CEDT found");

        let structures = cedt.get_structure_headers();
        info!("Numer of CEDT Structures: {}", structures.len());

        for s in structures {
            info!("{:?}", s);
        }

        let maybe_device: Option<&RwLock<EndpointHeader>> =
            pci_bus().search_by_ids(0x1b36, 0x000b).pop();

        if maybe_device.is_none() {
            info!("No PCIe device found");
            return;
        }

        let mut expander = maybe_device.unwrap().write();

        expander.update_command(pci_bus().config_space(), |command| {
            command.bitor(CommandRegister::BUS_MASTER_ENABLE | CommandRegister::MEMORY_ENABLE)
        });

        for s in 0..6 {
            match expander.bar(s, pci_bus().config_space()) {
                Some(bar) => info!("Bar {}: {:x}", s, bar.unwrap_io()),
                None => info!("Not Bar {}", s),
            }
        }
    } else {
        error!("No CEDT table found!");
    }
}
