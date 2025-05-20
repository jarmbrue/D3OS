use acpi::{
    AcpiTable,
    sdt::{SdtHeader, Signature},
};
use alloc::vec::Vec;
use core::{fmt::Debug, ptr, slice};
use log::{error, info};

use crate::acpi_tables;

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
    //CXIMS = 2,
    //RDPAS = 3,
    //CSDS = 4,
}

impl CedtStructureType {
    fn record_length(&self) -> usize {
        match &self {
            Self::CHBS(s) => s.record_length.into(),
            Self::CFMWS(s) => s.record_length.into(),
        }
    }

    fn is_valid(&self) -> bool {
        match &self {
            Self::CHBS(s) => s.is_valid(),
            Self::CFMWS(s) => s.is_valid(),
        }
    }
}

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

impl CXLHostBridgeStructure {
    fn is_valid(&self) -> bool {
        self.record_length == 0x20
    }
}

impl Debug for CXLHostBridgeStructure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let record_length = self.record_length;
        let uid = self.uid;
        let cxl_version = self.cxl_version;
        let base = self.base;
        let length = self.length;

        f.debug_struct("CXLHostBridgeStructure")
            .field("record_length", &record_length)
            .field("uid", &uid)
            .field("cxl_version", &cxl_version)
            .field("base", &base)
            .field("length", &length)
            .finish()
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

impl Debug for CXLFixedMemoryWindowStructure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let record_length = self.record_length;
        let base_hpa = self.base_hpa;
        let window_size = self.window_size;
        let interleave_arithmetic = self.interleave_arithmetic;
        let hbig = self.hbig;
        let window_restrictions = self.window_restrictions;
        let qtg_id = self.qtg_id;
        f.debug_struct("CXLFixedMemoryWindowStructure")
            .field("record_length", &record_length)
            .field("base_hpa", &base_hpa)
            .field("window_size", &window_size)
            .field("niw", &self.niw())
            .field("interleave_arithmetic", &interleave_arithmetic)
            .field("hbig", &hbig)
            .field("window_restrictions", &window_restrictions)
            .field("qtg_id", &qtg_id)
            .field("interleave_targets", &self.interleave_targets())
            .finish()
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
    } else {
        error!("No CEDT table found!");
    }
}
