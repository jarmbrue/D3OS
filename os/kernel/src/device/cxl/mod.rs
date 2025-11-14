pub mod capabilities;

use core::fmt::Debug;
use core::{slice, u64, usize};

use crate::device::acpi::cedt::{
    CEDT, CEDTStructureType, CXLFixedMemoryWindowStructure, CXLHostBridgeStructure,
};
use crate::device::cxl::DeviceRangeSizeLow::MEMORY_ACTIVE;
use crate::device::cxl::HdmDecoderCapabilityRegister::DECODER_COUNT;
use crate::device::cxl::RegisterLocatorBlockLow::{BIR, BLOCK_ID};
use crate::device::cxl::capabilities::{CXLCapability, CXLCapabilityIterator};
use crate::device::pci::{ConfigurationSpace, PciBus};
use crate::memory::vma::{VirtualMemoryArea, VmaType};
use crate::memory::{MemorySpace, PAGE_SIZE};
use crate::{acpi_tables, pci_bus, process_manager};
use acpi::AcpiTable;
use alloc::sync::Arc;
use alloc::vec;
use bit_field::BitField;
use bitfield_struct::bitfield;
use log::info;
use pci_types::capability::{PciCapability, PciCapabilityAddress};
use pci_types::{ConfigRegionAccess, PciAddress, PciHeader, PciPciBridgeHeader};
use tock_registers::interfaces::{Debuggable, Readable, Writeable};
use tock_registers::registers::{ReadOnly, ReadWrite};
use tock_registers::{register_bitfields, register_structs};
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

fn map_registers(chbs: &CXLHostBridgeStructure) -> PageRange {
    let address: u64 = chbs.base;
    let length: u64 = chbs.length;
    info!(
        "Found host bridge memory from cedt1 (Address: [0x{:x}], Length: [{} KB])",
        address,
        length / 1024
    );
    create_and_map_vam(address, length, "CXL_HB").range()
}

fn create_and_map_vam(address: u64, size: u64, label: &str) -> Arc<VirtualMemoryArea> {
    let start_page = Page::from_start_address(VirtAddr::new(address)).unwrap();
    let start_frame = PhysFrame::from_start_address(PhysAddr::new(address)).unwrap();
    let page_count = size / PAGE_SIZE as u64;

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
            label,
        )
        .expect("Not possible ot create VMA");

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

    vma
}

pub fn hexdump(address: *const u8, count: usize, chunk_size: usize, chunks_per_line: usize) {
    use log::info;

    for line_start in (0..count).step_by(chunk_size * chunks_per_line) {
        let mut line = [0u8; 256];
        let mut pos = 0;

        // Write address
        pos += format_hex_to_buf(
            &mut line[pos..],
            unsafe { address.add(line_start) } as usize,
            8,
        );
        line[pos] = b':';
        line[pos + 1] = b' ';
        pos += 2;

        // Write hex values
        for chunk_idx in 0..chunks_per_line {
            let byte_offset = line_start + chunk_idx * chunk_size;
            if byte_offset >= count {
                break;
            }

            match chunk_size {
                1 => {
                    let value = unsafe { address.add(byte_offset).read() as u8 };
                    pos += format_hex_to_buf(&mut line[pos..], value as usize, 2);
                }
                2 => {
                    let value = unsafe { (address.add(byte_offset) as *const u16).read() };
                    pos += format_hex_to_buf(&mut line[pos..], value as usize, 4);
                }
                4 => {
                    let value = unsafe { (address.add(byte_offset) as *const u32).read() };
                    pos += format_hex_to_buf(&mut line[pos..], value as usize, 8);
                }
                8 => {
                    let value = unsafe { (address.add(byte_offset) as *const u64).read() };
                    pos += format_hex_to_buf(&mut line[pos..], value as usize, 16);
                }
                _ => return,
            }
            line[pos] = b' ';
            pos += 1;
        }

        let line_str = core::str::from_utf8(&line[..pos]).unwrap_or("");
        info!("{}", line_str);
    }
}

fn format_hex_to_buf(buf: &mut [u8], mut value: usize, width: usize) -> usize {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    for i in 0..width {
        buf[width - 1 - i] = HEX_CHARS[value & 0xf];
        value >>= 4;
    }
    width
}

pub fn demo_host_bridge_capabilities(chbs: &CXLHostBridgeStructure) {
    let register_pages = map_registers(chbs);
    let primary_range = register_pages
        .clone()
        .nth(1)
        .expect("Primary Range not contained in CHBS range");

    for capability in CXLCapabilityIterator::new(primary_range.start_address().as_ptr::<u8>())
        .expect("There should be CXL Capabilities")
    //.filter(|c| c != &CXLCapability::Null)
    {
        info!("found capability: {:?}", capability);
        let maybe_address = match capability {
            CXLCapability::HDMDecorder(reg) => Some(reg.address),
            _ => None,
        };
        if let Some(address) = maybe_address {
            let decoder = unsafe {
                address
                    .cast::<HdmDecoderCapability>()
                    .as_ref()
                    .expect("should be possible to get ref")
            };
            info!("{:#?}", decoder.register.debug());
        }
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
    let data = unsafe {
        base_ptr
            .offset(CXL_ARB_MUX_REGISTER_OFFSET as isize)
            .cast::<u32>()
            .read()
    }; // Hier ist das Problem
    // auf das array kann nicht zugegriffen werden
    //info!("found data is: {:?}", data.get_cxlcachemem_primary_range());
    info!("found data is: {:x}", data);
}

#[bitfield(u32)]
struct LinkCapabiliesRegister {
    #[bits(4)]
    max_speed: u8,
    #[bits(6)]
    max_width: u8,
    support_for_l0s: bool,
    support_for_l1: bool,
    #[bits(3)]
    l0s_exit_latency: u8,
    #[bits(3)]
    l1_exit_latency: u8,
    #[bits(4)]
    _reserved_1: u8,
    optionality_compliance: bool,
    #[bits(9)]
    _reserved_2: u16,
}

#[bitfield(u32)]
struct LinkStatusControlRegister {
    control: u16,
    #[bits(4)]
    current_link_speed: u8,
    #[bits(6)]
    negotiated_link_width: u8,
    _reserved_1: bool,
    link_training: bool,
    slock_clock_config: bool,
    data_link_layer_link_active: bool,
    link_bandwidth_management_status: bool,
    link_autonomous_bandwidth_status: bool,
}

fn print_pci_capability(address: PciAddress, offset: u16, access: &ConfigurationSpace) {
    let link_register = LinkCapabiliesRegister(unsafe { access.read(address, offset + 0x0C) });
    let link_status_control =
        LinkStatusControlRegister(unsafe { access.read(address, offset + 0x10) });
    //info!("\t{:?}", link_register);
    //info!("\t{:?}", link_status_control);
    info!(
        "\tPCI Capability offset: {:x}, current speed={}",
        offset,
        link_status_control.current_link_speed()
    );
}

#[bitfield(u32)]
struct PciCapabilityHeader {
    id: u8,
    next_cap_offset: u8,
    extension: u16,
}

#[bitfield(u32)]
struct PciExtendedCapabilityHeader {
    id: u16,
    #[bits(4)]
    version: u8,
    #[bits(12)]
    next_cap_offset: u16,
}

enum CxlDvsec {
    Device(&'static mut CxlDvsecForDevice),
    NonCXLFunctions,
    Port(&'static mut CxlDvsecForPort),
    GPFPort,
    GPFDevice,
    GPFFlexBus(&'static mut CxlDvsecFlexBus), // access via RCRB for RCH-RCD
    RegisterLocator(&'static mut RegisterLocator),
    MLD,
    CXLDeviceTestCapabilityAdvertisement,
    Unknown,
}

register_bitfields![
    u16,

    // Device

    /// Defined in CXL Specification 3.2 Section 8.1.3.1
    DeviceCapability[
        CACHE                           OFFSET(0)  NUMBITS(1) [],
        IO                              OFFSET(1)  NUMBITS(1) [],
        MEM                             OFFSET(2)  NUMBITS(1) [],
        MEM_HW_INIT_MODE                OFFSET(3)  NUMBITS(1) [],
        HDM_COUNT                       OFFSET(4)  NUMBITS(2) [],
        CACHE_WRITEBACK_AND_INVALIDATE  OFFSET(6)  NUMBITS(1) [],
        CXL_RESET                       OFFSET(7)  NUMBITS(1) [],
        CXL_RESET_TIMEOUT               OFFSET(8)  NUMBITS(3) [],
        CXL_RESET_MEM_CLR               OFFSET(11) NUMBITS(1) [],
        TSP                             OFFSET(12) NUMBITS(1) [],
        MULTI_LOGICAL_DEVICE            OFFSET(13) NUMBITS(1) [],
        VIRAL                           OFFSET(14) NUMBITS(1) [],
        PM_INIT_COMPLETION_REPORTING    OFFSET(15) NUMBITS(1) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.2
    DeviceControl[
        CACHE_ENABLE          OFFSET(0)  NUMBITS(1) [],
        IO_ENABLE             OFFSET(1)  NUMBITS(1) [],
        MEM_ENABLE            OFFSET(2)  NUMBITS(1) [],
        CACHE_SF_COVERAGE     OFFSET(3)  NUMBITS(5) [],
        CACHE_SF_GRANULARITY  OFFSET(8)  NUMBITS(3) [],
        CACHE_CLEAN_EVICTION  OFFSET(11) NUMBITS(1) [],
        DIRECT_P2P_MEM_ENABLE OFFSET(12) NUMBITS(1) [],
        VIRAL_ENABLE          OFFSET(14) NUMBITS(1) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.3
    DeviceStatus2[
        VIRAL_ENABLED          OFFSET(14) NUMBITS(1) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.4
    DeviceControl2[
        DIABLE_CACHING                             OFFSET(0) NUMBITS(1) [],
        INITIATE_CACHE_WRITE_BACK_AND_INIVALIDATE  OFFSET(1) NUMBITS(1) [],
        INITIATE_RESET                             OFFSET(2) NUMBITS(1) [],
        CXL_RESET_MEM_CLR_ENABLE                   OFFSET(3) NUMBITS(1) [],
        DESIRED_VOLATILE_HDM_STATE_AFTER_HOT_RESET OFFSET(4) NUMBITS(1) [],
        MODIFIED_COMPLETION_ENABLE                 OFFSET(5) NUMBITS(1) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.5
    DeviceStatus[
        CACHE_INVALID                            OFFSET(0)  NUMBITS(1) [],
        CXL_RESET_COMPLETE                       OFFSET(1)  NUMBITS(1) [],
        CXL_RESET_ERROR                          OFFSET(2)  NUMBITS(1) [],
        VOLATILE_HDM_PRESERVATION_ERROR          OFFSET(3)  NUMBITS(1) [],
        POWER_MANAGEMENT_INITIALIZATION_COMPLETE OFFSET(15) NUMBITS(1) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.6
    DeviceLock[
        CONFIG_LOCK OFFSET(0) NUMBITS(1) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.7
    DeviceCapability2[
        CACHE_SIZE_UNIT             OFFSET(0) NUMBITS(4) [
            NotReported = 0x0,
            Size64KB = 0x1,
            Size1MB = 0x2,
        ],
        FALLBACK_CAPABILITY         OFFSET(4) NUMBITS(2) [
            NotSupported = 0b00,
            PCIe = 0b01,
            CxlType1 = 0b10,
            CxlType3 = 0b11,
        ],
        MODIFIED_COMPLETION_CAPABLE OFFSET(6) NUMBITS(1) [],
        NO_CLEAN_WRITEBACK          OFFSET(7) NUMBITS(1) [],
        CACHE_SIZE                  OFFSET(8) NUMBITS(8) [],
    ],

    /// Defined in CXL Specification 3.2 Section 8.1.3.7
    DeviceCapability3[
        DEFAULT_VOLATILE_HDM_STATE_AFTER_COLD_RESET        OFFSET(0) NUMBITS(1) [],
        DEFAULT_VOLATILE_HDM_STATE_AFTER_WARM_RESET        OFFSET(1) NUMBITS(1) [],
        DEFAULT_VOLATILE_HDM_STATE_AFTER_HOT_RESET         OFFSET(2) NUMBITS(1) [],
        VOLATILE_HDM_STATE_AFTER_HOT_RESET_CONFIGURABILITY OFFSET(3) NUMBITS(1) [],
        DIRECT_P2P_MEM_CAPABLE                             OFFSET(4) NUMBITS(1) [],
    ],


    // FlexBus

    /// Defined in CXL Specification 3.2 Section 8.2.1.3.1
    FlexBusCapability[
        CACHE                           0,
        IO                              1,
        MEM                             2,
        CXL_FLIT_68_AND_VH              5,
        CXL_MULTI_LOGICAL_DEVICE        6,
        CXL_LATENCY_OPTIMIZED_256B_FLIT 13,
        CXL_PDR_FLIT                    14,
    ],

    /// Defined in CXL Specification 3.2 Section 8.2.1.3.1
    FlexBusControl[
        CACHE_ENABLED                           0,
        IO_ENABLED                              1,
        MEM_ENABLED                             2,
        CXL_SYNC_HDR_BYPASS_ENABLED             3,
        DRIFT_BUFFER_ENABLED                    4,
        CXL_FLIT_68_AND_VH_ENABLED              5,
        CXL_MULTI_LOGICAL_DEVICE_ENABLED        6,
        DISABLE_RCD_TRAINING                    7,
        RETIMER_1_PRESENT                       8,
        RETIMER_2_PRESENT                       9,
        CXL_LATENCY_OPTIMIZED_256B_FLIT_ENABLED 13,
        CXL_PDR_FLIT_ENABLED                    14,
    ],

    /// Defined in CXL Specification 3.2 Section 8.2.1.3.1
    FlexBusStatus[
        CACHE_ENABLED                               0,
        IO_ENABLED                                  1,
        MEM_ENABLED                                 2,
        CXL_SYNC_HDR_BYPASS_ENABLED                 3,
        DRIFT_BUFFER_ENABLED                        4,
        CXL_FLIT_68_AND_VH_ENABLED                  5,
        CXL_MULTI_LOGICAL_DEVICE_ENABLED            6,
        EVEN_HALF_FAILED                            7,
        CXL_CORRECTABLE_PROTOCOL_ID_FRAMING_ERROR   8,
        CXL_UNCORRECTABLE_PROTOCOL_ID_FRAMING_ERROR 9,
        CXL_UNEXPECTED_PROTOCOL_ID_FRAMING_ERROR    10,
        CXL_RETIMERS_PRESENT_MISMATCHED             11,
        FLEX_BUS_ENABLED_BITS_PHASE_2_MISMATCH      11,
        CXL_LATENCY_OPTIMIZED_256B_FLIT_ENABLED     13,
        CXL_PDR_FLIT_ENABLED                        14,
        CXL_IO_THROTTLE_REQUIRED_AT_64_GTS          14,
    ],

];

register_bitfields![
u32,

    /// Defined in CXL Specification 3.2 Section 8.1.3.8.2 and 8.1.3.8.6
    DeviceRangeSizeLow[
        MEMORY_INFO_VALID      OFFSET(0) NUMBITS(1) [],
        MEMORY_ACTIVE          OFFSET(1) NUMBITS(1) [],
        MEDIA_TYPE             OFFSET(2) NUMBITS(3) [],
        MEMORY_CLASS           OFFSET(5) NUMBITS(3) [],
        DESIRED_INTERLEAVE     OFFSET(8) NUMBITS(5) [],
        MEMORY_ACTIVE_TIMEOUT  OFFSET(13) NUMBITS(3) [],
        MEMORY_ACTIVE_DEGRADED OFFSET(16) NUMBITS(1) [],
        MEORY_SIZE_LOW         OFFSET(28) NUMBITS(4) [],
    ],


    /// Defined in CXL Specification 3.2 Section 8.1.9.1
    RegisterLocatorBlockLow[
        BIR      OFFSET(0)  NUMBITS(3) [],
        BLOCK_ID OFFSET(8)  NUMBITS(8) [
            Empty                             = 0x00,
            ComponentRegisters                = 0x01,
            BarVirtualizationAclRegisters     = 0x02,
            CpmuRegisters                     = 0x03,
            ChmuRegisters                     = 0x04,
            DesignatedVendorSpecificRegisters = 0xFF,
        ],
        OFFSET      OFFSET(16)  NUMBITS(16) [],
    ],


    // Decoder

    HdmDecoderCapabilityRegister[
        DECODER_COUNT                           OFFSET(0)  NUMBITS(4) [],
        TARGET_COUNT                            OFFSET(5)  NUMBITS(3) [],
        A11TO8INTERLEAVE_CAPABLE                OFFSET(8)  NUMBITS(1) [],
        A14TO12INTERLEAVE_CAPABLE               OFFSET(9)  NUMBITS(1) [],
        POISON_ON_DECODE_ERROR_CAPABILITY       OFFSET(10) NUMBITS(1) [],
        THREE_SIX_TWELVE_WAY_INTERLEAVE_CAPABLE OFFSET(11) NUMBITS(1) [],
        SIXTEEN_WAY_INTERLEAVE_CAPABLE          OFFSET(12) NUMBITS(1) [],
        UIO_CAPABLE                             OFFSET(13) NUMBITS(1) [],
        UIO_CAPABLE_DECODER_COUNT               OFFSET(16) NUMBITS(4) [],
        MEM_DATA_NXM_CAPABLE                    OFFSET(20) NUMBITS(1) [],
        SUPPORTED_COHERENCY_MODELS              OFFSET(21) NUMBITS(2) [
            Unknown = 0x0,
            Device = 0x1,
            HostOnly = 0x2,
            HostOnlyOrDevice = 0x3,
        ],
    ],

    HdmDecoderGlobalControlRegister [
        CXL_CAPABILITY_ID 0,
        HDM_DECODER_ENABLE 1,
    ],

    HdmDecoderControl [
        INTERLEAVE_GRANULARITY          OFFSET(0)  NUMBITS(4) [],
        INTERLEAVE_WAYS                 OFFSET(4)  NUMBITS(4) [],
        LOCK_ON_COMMIT                  OFFSET(8)  NUMBITS(1) [],
        COMMIT                          OFFSET(9)  NUMBITS(1) [],
        COMMITTED                       OFFSET(10) NUMBITS(1) [],
        ERROR_NO_COMMITTED              OFFSET(11) NUMBITS(1) [],
        TARGET_RANGE_TYPE               OFFSET(12) NUMBITS(1) [],
        BI                              OFFSET(13) NUMBITS(1) [],
        UIO                             OFFSET(14) NUMBITS(1) [],
        UPSTREAM_INTERLEAVE_GRANULARITY OFFSET(16) NUMBITS(4) [],
        UPSTREAM_INTERLEAVE_WAYS        OFFSET(20) NUMBITS(4) [],
        INTERLEAVE_SET_POSITION         OFFSET(24) NUMBITS(4) [],
    ],
];

register_structs! {
    CxlDvsecForDevice {
        (0x00 => pci_ext_cap: ReadOnly<u32>),
        (0x04 => dvsec_1: ReadOnly<u32>),
        (0x08 => dvsec_2: ReadOnly<u16>),
        (0x0A => capability: ReadOnly<u16, DeviceCapability::Register>),
        (0x0C => control: ReadOnly<u16, DeviceControl::Register>),
        (0x0E => status: ReadOnly<u16, DeviceStatus::Register>),
        (0x10 => control_2: ReadOnly<u16, DeviceControl2::Register>),
        (0x12 => status_2: ReadOnly<u16, DeviceStatus2::Register>),
        (0x14 => lock: ReadOnly<u16, DeviceLock::Register>),
        (0x16 => capability_2: ReadOnly<u16, DeviceCapability2::Register>),
        (0x18 => range_1_size_high: ReadOnly<u32>),
        (0x1C => range_1_size_low: ReadOnly<u32, DeviceRangeSizeLow::Register>),
        (0x20 => range_1_base_high: ReadWrite<u32>),
        (0x24 => range_1_base_low: ReadWrite<u32>),
        (0x28 => range_2_size_high: ReadOnly<u32>),
        (0x2C => range_2_size_low: ReadOnly<u32, DeviceRangeSizeLow::Register>),
        (0x30 => range_2_base_high: ReadWrite<u32>),
        (0x34 => range_2_base_low: ReadWrite<u32>),
        (0x38 => capability_3: ReadOnly<u16, DeviceCapability3::Register>),
        (0x3A => _reserved),
        (0x3C => @END),
    },

    CxlDvsecForPort {
        (0x00 => pci_ext_cap: ReadOnly<u32>),
        (0x04 => dvsec_1: ReadOnly<u32>),
        (0x08 => dvsec_2: ReadOnly<u16>),
        (0x0A => cxl_port_ext_status: ReadOnly<u16>),
        (0x0C => port_control_ext: ReadOnly<u16>),
        (0x0E => alt_bus_base: ReadOnly<u8>),
        (0x0F => alt_bus_limi: ReadOnly<u8>),
        (0x10 => alt_mem_base: ReadOnly<u16>),
        (0x12 => alt_mem_limit: ReadOnly<u16>),
        (0x14 => alt_pref_mem_base: ReadOnly<u16>),
        (0x16 => alt_pref_mem_limit: ReadOnly<u16>),
        (0x18 => alt_prefetchable_mem_base: ReadOnly<u32>),
        (0x1C => alt_prefetchable_mem_limit: ReadOnly<u32>),
        (0x20 => cxl_rcrb_base: [ReadOnly<u32>; 2]),
        (0x28 => @END),
    },

    CxlDvsecFlexBus {
        (0x00 => pci_ext_cap: ReadOnly<u32>),
        (0x04 => dvsec_1: ReadOnly<u32>),
        (0x08 => dvsec_2: ReadOnly<u16>),
        (0x0A => capability: ReadOnly<u16, FlexBusCapability::Register>),
        (0x0C => control: ReadOnly<u16, FlexBusControl::Register>),
        (0x0E => status: ReadOnly<u16, FlexBusStatus::Register>),
        (0x10 => received_modified_ts_data_phase_1: ReadOnly<u32>),
        (0x14 => cap_2: ReadOnly<u32>),
        (0x18 => control_2: ReadOnly<u32>),
        (0x1C => status_2: ReadOnly<u32>),
        (0x20 => @END),
    },

    RegisterLocator {
        (0x00 => pci_ext_cap: ReadOnly<u32>),
        (0x04 => dvsec_1: ReadOnly<u32>),
        (0x08 => dvsec_2: ReadOnly<u16>),
        (0x0A => _reserved),
        (0x0C => register_block_1_low: ReadOnly<u32, RegisterLocatorBlockLow::Register>),
        (0x10 => register_block_1_high: ReadOnly<u32>),
        (0x14 => register_block_2_low: ReadOnly<u32, RegisterLocatorBlockLow::Register>),
        (0x18 => register_block_2_high: ReadOnly<u32>),
        (0x1C => register_block_3_low: ReadOnly<u32, RegisterLocatorBlockLow::Register>),
        (0x20 => register_block_3_high: ReadOnly<u32>),
        (0x24 => @END),
    },

    HdmDecoderCapability {
        (0x00 => register: ReadOnly<u32, HdmDecoderCapabilityRegister::Register>),
        (0x04 => global_control: ReadOnly<u32, HdmDecoderGlobalControlRegister::Register>),
        (0x08 => _reserved),
        (0x10 => @END),
    },

    HdmDecoderEntry {
        (0x00 => base_low: ReadOnly<u32>),
        (0x04 => base_high: ReadOnly<u32>),
        (0x08 => size_low: ReadOnly<u32>),
        (0x0C => size_high: ReadOnly<u32>),
        (0x10 => control: ReadOnly<u32, HdmDecoderControl::Register>),
        (0x14 => target_low: ReadOnly<u32>),
        (0x18 => target_high: ReadOnly<u32>),
        (0x1C => _reserved),
        (0x20 => @END),
    }

}

struct HdmRange {
    base: u64,
    size: u64,
}

impl Debug for HdmRange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("0x{:016x}[0x{:x}]", self.base, self.size))
    }
}

impl CxlDvsecForDevice {
    fn range_1(&self) -> HdmRange {
        let base = (self.range_1_base_high.get() as u64) << 32
            | (self.range_1_base_low.get() as u64) & 0xF000_0000;
        let size = (self.range_1_size_high.get() as u64) << 32
            | (self.range_1_size_low.get() as u64) & 0xF000_0000;
        HdmRange { base, size }
    }

    fn range_2(&self) -> HdmRange {
        let base = (self.range_2_base_high.get() as u64) << 32
            | (self.range_2_base_low.get() as u64) & 0xF000_0000;
        let size = (self.range_2_size_high.get() as u64) << 32
            | (self.range_2_size_low.get() as u64) & 0xF000_0000;
        HdmRange { base, size }
    }
}

impl HdmDecoderEntry {
    fn base(&self) -> u64 {
        (self.base_high.get() as u64) << 32 | (self.base_low.get() as u64) & 0xF000_0000
    }

    fn size(&self) -> u64 {
        (self.size_high.get() as u64) << 32 | (self.size_low.get() as u64) & 0xF000_0000
    }
}

impl HdmDecoderCapability {
    fn decoder_count(&self) -> u8 {
        match self.register.read(DECODER_COUNT) {
            0x0 => 1,
            0x1 => 2,
            0x2 => 4,
            0x3 => 6,
            0x4 => 8,
            0x5 => 10,
            0x6 => 12,
            0x7 => 14,
            0x8 => 16,
            0x9 => 20,
            0xa => 24,
            0xb => 28,
            0xc => 32,
            _ => 0,
        }
    }
    fn decoders(&'static self) -> &'static [HdmDecoderEntry] {
        let ptr = self as *const HdmDecoderCapability;
        unsafe { slice::from_raw_parts(ptr.offset(1).cast(), self.decoder_count() as usize) }
    }
}

impl CxlDvsecForPort {
    fn is_cxl_rcrb_enabled(&self) -> bool {
        self.cxl_rcrb_base[0].get().get_bit(0)
    }

    fn get_cxl_rcrb_base(&self) -> u64 {
        (self.cxl_rcrb_base[1].get() as u64) << 32
            | (self.cxl_rcrb_base[0].get() & 0xFFFFF000) as u64
    }
}

impl RegisterLocator {
    fn register_block_1_offset(&self) -> u64 {
        (self.register_block_1_high.get() as u64) << 32
            | (self.register_block_1_low.get() & 0xFFFF0000) as u64
    }

    fn register_block_2_offset(&self) -> u64 {
        (self.register_block_2_high.get() as u64) << 32
            | (self.register_block_2_low.get() & 0xFFFF0000) as u64
    }

    fn register_block_3_offset(&self) -> u64 {
        (self.register_block_3_high.get() as u64) << 32
            | (self.register_block_3_low.get() & 0xFFFF0000) as u64
    }
}

impl CxlDvsec {
    pub fn parse(address: PciCapabilityAddress, access: &ConfigurationSpace) -> Option<CxlDvsec> {
        let data = unsafe { access.read(address.address, address.offset + 8) };
        let reg_ptr =
            access.physical_address_with_offset(address.address, address.offset) as *mut u8;
        unsafe {
            match data.get_bits(0..16) {
                0x0 => reg_ptr
                    .cast::<CxlDvsecForDevice>()
                    .as_mut()
                    .map(|f| Self::Device(f)),
                0x2 => Some(Self::NonCXLFunctions),
                0x3 => reg_ptr
                    .cast::<CxlDvsecForPort>()
                    .as_mut()
                    .map(|f| Self::Port(f)),
                0x4 => Some(Self::GPFPort),
                0x5 => Some(Self::GPFDevice),
                0x7 => reg_ptr
                    .cast::<CxlDvsecFlexBus>()
                    .as_mut()
                    .map(|f| Self::GPFFlexBus(f)),
                0x8 => reg_ptr
                    .cast::<RegisterLocator>()
                    .as_mut()
                    .map(|f| Self::RegisterLocator(f)),
                0x9 => Some(Self::MLD),
                0xA => Some(Self::CXLDeviceTestCapabilityAdvertisement),
                _ => Some(Self::Unknown),
            }
        }
    }
}

impl Debug for CxlDvsec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Device(_) => f.write_str("Device"),
            Self::NonCXLFunctions => f.write_str("NonCXLFunctions"),
            Self::Port(_) => f.write_str("Port"),
            Self::GPFPort => f.write_str("GPFPort"),
            Self::GPFDevice => f.write_str("GPFDevice"),
            Self::GPFFlexBus(_) => f.write_str("GPFFlexBus"), // access via RCRB for RCH-RCD
            Self::RegisterLocator(_) => f.write_str("RegisterLocator"),
            Self::MLD => f.write_str("MLD"),
            Self::CXLDeviceTestCapabilityAdvertisement => {
                f.write_str("CXLDeviceTestCapabilityAdvertisement")
            }
            Self::Unknown => f.write_str("Unknown"),
        }
    }
}

fn print_dvsec_capability(dvsec: &CxlDvsec) {
    match dvsec {
        CxlDvsec::Device(v) => info!(
            "\tCXL DVSEC for Device:\nRange 1: 0x{:?}\n{:#?}\nRange 2: 0x{:?}\n{:#?}\n{:#?}\n{:#?}\n{:#?}",
            v.range_1(),
            v.range_1_size_low.debug(),
            v.range_2(),
            v.range_2_size_low.debug(),
            v.capability.debug(),
            v.capability_2.debug(),
            v.capability_3.debug()
        ),
        CxlDvsec::Port(v) => {
            info!(
                "\tCXL DVSEC for Port: rcrb_base={:016x}",
                v.get_cxl_rcrb_base()
            )
        }
        CxlDvsec::GPFFlexBus(v) => info!(
            "CXL GPF FlexBus: cap1={:#?}, cap2={:#?}, status1={:#?}, status2={:#?}, control1={:#?}, control2={:#?}",
            v.capability.debug(),
            v.cap_2.get(),
            v.status.debug(),
            v.status_2.get(),
            v.control.debug(),
            v.control_2.get()
        ),
        CxlDvsec::RegisterLocator(v) => {
            info!("CXL RegisterLocator:");
            if let Some(type_id) = v
                .register_block_1_low
                .read_as_enum::<BLOCK_ID::Value>(BLOCK_ID)
                && type_id != BLOCK_ID::Value::Empty
            {
                info!(
                    "\tBAR {}: {:?} [0x{:016x}]",
                    v.register_block_1_low.read(BIR),
                    type_id,
                    v.register_block_1_offset()
                );
            }

            if let Some(type_id) = v
                .register_block_2_low
                .read_as_enum::<BLOCK_ID::Value>(BLOCK_ID)
                && type_id != BLOCK_ID::Value::Empty
            {
                info!(
                    "\tBAR {}: {:?} [0x{:016x}]",
                    v.register_block_2_low.read(BIR),
                    type_id,
                    v.register_block_2_offset()
                );
            }

            if let Some(type_id) = v
                .register_block_3_low
                .read_as_enum::<BLOCK_ID::Value>(BLOCK_ID)
                && type_id != BLOCK_ID::Value::Empty
            {
                info!(
                    "\tBAR {}: {:?} [0x{:016x}]",
                    v.register_block_3_low.read(BIR),
                    type_id,
                    v.register_block_3_offset()
                );
            }
        }
        id => info!("\tCXL DVSEC {:?}", id),
    }
}

fn print_component_registers(address: u64, size: u64) {
    create_and_map_vam(address, size, "BAR0");

    info!("CXL.cachemem Primary Range");

    //hexdump((address + 0x1000) as *const u8, 0x1000, 4, 16);
    for capability in CXLCapabilityIterator::new((address + 0x1000) as *const u8)
        .expect("There should be CXL Capabilities")
    //.filter(|c| c != &CXLCapability::Null)
    {
        info!("found capability: {:?}", capability);
        let maybe_address = match capability {
            CXLCapability::HDMDecorder(reg) => Some(reg.address),
            _ => None,
        };
        if let Some(address) = maybe_address {
            let decoder_cap = unsafe {
                address
                    .cast::<HdmDecoderCapability>()
                    .as_ref()
                    .expect("Should be possible to get ref")
            };
            info!("{:#?}", decoder_cap.register.debug());
            info!("{:#?}", decoder_cap.global_control.debug());
            for (i, d) in decoder_cap.decoders().iter().enumerate() {
                info!(
                    "Decoder {}: base={:016x} size={:016x}\n{:#?}",
                    i,
                    d.base(),
                    d.size(),
                    d.control.debug()
                );
            }
        }
    }
}

struct CxlDevice {
    address: PciAddress,
    device_dvsec: CxlDvsecForDevice
}

impl CxlDevice {
    fn from_address(address: PciAddress, access: &ConfigurationSpace) {

    }
}

fn setup_memory_range(device: &CxlDvsecForDevice) {
    let next_free_base: u64 = 0x40_0000_0000;
    if device.range_1_size_low.is_set(MEMORY_ACTIVE) && device.range_1().base == 0 {
        info!("setting dev base to 0x{:016x}", next_free_base);
        device.range_1_base_low.set(next_free_base as u32);
        device.range_1_base_high.set((next_free_base >> 32) as u32);
    }
}

fn print_capabilities(header: PciHeader, access: &ConfigurationSpace) {
    let address = header.address();
    if !header.status(access).has_capability_list() {
        info!("{} has no Capabilities", header.address());
    }
    let data = unsafe { access.read(address, 0x34) };
    let mut offset = (data & 0xFF) as u16;

    info!("Capabilities for {}", address);
    while offset != 0 {
        let data = PciCapabilityHeader(unsafe { access.read(address, offset) });
        match data.id() {
            0x10 => print_pci_capability(address, offset, access),
            _ => info!("\tUnknown id={:x}, offset={:x}", data.id(), offset),
        }
        offset = data.next_cap_offset() as u16;
    }

    info!("Extended Capabilities For {}", address);

    offset = 0x100;
    while offset != 0 {
        let cap_header = PciExtendedCapabilityHeader(unsafe { access.read(address, offset) });
        match cap_header.id() {
            0x23 => {
                let dvsec = CxlDvsec::parse(
                    PciCapabilityAddress {
                        address: address,
                        offset: offset,
                    },
                    access,
                )
                .expect("Expected CXL DVSEC");

                if let CxlDvsec::Device(d) = &dvsec {
                    setup_memory_range(d);
                }

                print_dvsec_capability(&dvsec);
            }
            _ => info!("\tUnknown id={:x}, offset={:x}", cap_header.id(), offset),
        }
        offset = cap_header.next_cap_offset();
    }
}

fn debug_host_bridge(hb: &CXLHostBridgeStructure) {
    info!("Host Bridge ist {:#?}", hb);
    demo_host_bridge_capabilities(hb);
    info!("Host Bridge hat die folgenden Root Ports:");
    let cxl_bus = PciBus::scan_by_nr(hb.uid as u8, pci_bus().config_space()); // TODO: lookup _BBN in ACPI table instead of using _UID directly
    let root_port = PciPciBridgeHeader::from_header(
        PciHeader::new(PciAddress::new(0x8000, hb.uid as u8, 0, 0)),
        cxl_bus.config_space(),
    )
    .expect("There should be an PciPciBridge");
    let dev = cxl_bus
        .search_by_ids(0x8086, 0x0d93)
        .first()
        .unwrap()
        .read();
    print_capabilities(root_port.header(), cxl_bus.config_space());
    print_capabilities(dev.header(), cxl_bus.config_space());

    {
        let address1 = unsafe {
            cxl_bus
                .config_space()
                .read(root_port.header().address(), 0x10)
        };
        let address2 = unsafe {
            cxl_bus
                .config_space()
                .read(root_port.header().address(), 0x14)
        };
        let address = (address2 as u64) << 32 | ((address1 as u64) & 0xFFFFF000);
        info!("BAR0 CXL Bridge at 0x{:016x}", address);
        print_component_registers(address as u64, 0x2000 as u64);
    }

    {
        let (address, size) = dev.bar(0, cxl_bus.config_space()).unwrap().unwrap_mem();
        info!("BAR0 CXL Device at 0x{:016x}", address);
        print_component_registers(address as u64, size as u64);
    }
}

fn test_hdm(base: u64, size: u64) {
    create_and_map_vam(base, size, "CXL_MEM");
    let ptr = base as *mut u8;
    //let mut vec = vec![0; 255];
    //let ptr = vec.as_mut_ptr();
    for i in 0..=255 {
        unsafe { ptr.offset(i as isize).write(i) };
    }

    hexdump(ptr, 0x100, 1, 16);
}


pub fn init() {
    if let Ok(cedt) = acpi_tables().lock().find_table::<CEDT>() {
        info!("Found CEDT table {:?}", cedt.header());
        let structures = cedt.get_structures();
        for structure in structures {
            match structure.typ {
                CEDTStructureType::CXLHostBridgeStructure => {
                    debug_host_bridge(structure.as_structure());
                    test_hdm(0x40_0000_0000, 0x2000_0000);
                }
                CEDTStructureType::CXLFixedMemoryWindowStructure => {
                    let s = structure.as_structure::<CXLFixedMemoryWindowStructure>();
                    info!("Memory Window ist {:#?}", s);
                    test_hdm(s.base_hpa, s.window_size);
                }
                _ => info!("found different structure"),
            }
        }
        //TODO: map CXL Memory Windows
    }
}
