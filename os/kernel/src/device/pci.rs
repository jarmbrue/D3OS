use acpi::mcfg::Mcfg;
use alloc::vec::Vec;
use log::info;
use pci_types::{
    BaseClass, ConfigRegionAccess, EndpointHeader, HeaderType, PciAddress, PciHeader,
    PciPciBridgeHeader, SubClass,
};
use spin::RwLock;
use x86_64::{structures::paging::{frame::PhysFrameRange, Page, PageTableFlags, PhysFrame}, PhysAddr, VirtAddr};

use crate::{acpi_tables, memory::{vma::VmaType, MemorySpace}, process_manager};

const MAX_DEVICES_PER_BUS: u8 = 32;
const MAX_FUNCTIONS_PER_DEVICE: u8 = 8;
const INVALID: u16 = 0xffff;


pub struct PciBus {
    config_space: &'static ConfigurationSpace,
    devices: Vec<RwLock<EndpointHeader>>,
}

pub struct ConfigurationSpace {
    base_address: u64,
}

impl ConfigurationSpace {
    const fn new(base_address: u64) -> Self {
        Self { base_address }
    }

    pub fn from_mcfg() -> Self {
        let mcfg = acpi_tables()
            .lock()
            .find_table::<Mcfg>()
            .expect("No MCFG table found");
        info!("{:?}", *mcfg);
        let mcfg_entry = mcfg.entries().get(0).expect("MCFG has no entry");

        let start_page = Page::from_start_address(VirtAddr::new(mcfg_entry.base_address)).unwrap();
        let start_frame = PhysFrame::from_start_address(PhysAddr::new(mcfg_entry.base_address)).unwrap();
        let page_count = 256*32*8;

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
                "PCI",
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

        return Self::new(mcfg_entry.base_address);
    }

    pub fn physical_address_with_offset(&self, address: PciAddress, offset: u16) -> u64 {
        self.base_address
            + ((address.bus() as u64) << 20)
            + ((address.device() as u64) << 15)
            + ((address.function() as u64) << 12)
            + offset as u64
    }
}

impl ConfigRegionAccess for ConfigurationSpace {
    unsafe fn read(&self, address: PciAddress, offset: u16) -> u32 {
        unsafe { core::ptr::read_volatile(self.physical_address_with_offset(address, offset) as *const u32) }
    }

    unsafe fn write(&self, address: PciAddress, offset: u16, value: u32) {
        unsafe {
            core::ptr::write_volatile(self.physical_address_with_offset(address, offset) as *mut u32, value)
        }
    }
}

impl PciBus {
    pub fn scan(config_space: &'static ConfigurationSpace) -> Self {
        let mut pci = Self {
            config_space: config_space,
            devices: Vec::new(),
        };

        let root = PciHeader::new(PciAddress::new(0x8000, 0, 0, 0));
        if root.has_multiple_functions(&pci.config_space) {
            info!("Multiple PCI host controllers detected");
            for i in 0..MAX_FUNCTIONS_PER_DEVICE {
                let address = PciAddress::new(0x8000, 0, 0, i);
                let header = PciHeader::new(address);
                if header.id(&pci.config_space).0 == INVALID {
                    break;
                }

                pci.scan_bus(address);
            }
        } else {
            info!("Single PCI host controller detected");
            pci.scan_bus(PciAddress::new(0x8000, 0, 0, 0));
        }

        pci
    }

    pub fn scan_by_nr(bus_nr: u8, config_space: &'static ConfigurationSpace) -> Self {
        let mut pci = Self {
            config_space: config_space,
            devices: Vec::new(),
        };
        let root = PciHeader::new(PciAddress::new(0x8000, bus_nr, 0, 0));
        if root.has_multiple_functions(&pci.config_space) {
            info!("Multiple PCI host controllers detected");
            for i in 0..MAX_FUNCTIONS_PER_DEVICE {
                let address = PciAddress::new(0x8000, bus_nr, 0, i);
                let header = PciHeader::new(address);
                if header.id(&pci.config_space).0 == INVALID {
                    break;
                }

                pci.scan_bus(address);
            }
        } else {
            info!("Single PCI host controller detected on non 0 pci bus");
            pci.scan_bus(PciAddress::new(0x8000, bus_nr, 0, 0));
        }

        return pci;
    }

    #[inline(always)]
    pub fn config_space(&self) -> &ConfigurationSpace {
        &self.config_space
    }

    pub fn search_by_ids(&self, vendor_id: u16, device_id: u16) -> Vec<&RwLock<EndpointHeader>> {
        self.devices
            .iter()
            .filter(|device| {
                device.read().header().id(self.config_space()) == (vendor_id, device_id)
            })
            .collect()
    }

    pub fn search_by_class(
        &self,
        base_class: BaseClass,
        sub_class: SubClass,
    ) -> Vec<&RwLock<EndpointHeader>> {
        self.devices
            .iter()
            .filter(|device| {
                let info = device
                    .read()
                    .header()
                    .revision_and_class(self.config_space());
                info.1 == base_class && info.2 == sub_class
            })
            .collect()
    }

    fn scan_bus(&mut self, address: PciAddress) {
        assert_eq!(address.device(), 0);
        assert_eq!(address.function(), 0);

        for i in 0..MAX_DEVICES_PER_BUS {
            self.check_device(PciAddress::new(address.segment(), address.bus(), i, 0));
        }
    }

    fn check_device(&mut self, address: PciAddress) {
        assert_eq!(address.function(), 0);

        let device = PciHeader::new(address);
        let id = device.id(self.config_space());
        if id.0 == INVALID {
            return;
        }

        self.check_function(address);

        if device.has_multiple_functions(self.config_space()) {
            for i in 1..MAX_FUNCTIONS_PER_DEVICE {
                let address =
                    PciAddress::new(address.segment(), address.bus(), address.device(), i);
                let device = PciHeader::new(address);
                if device.id(self.config_space()).0 == INVALID {
                    break;
                }

                self.check_function(address)
            }
        }
    }

    fn check_function(&mut self, address: PciAddress) {
        let device = PciHeader::new(address);
        let id = device.id(self.config_space());

        if device.header_type(self.config_space()) == HeaderType::PciPciBridge {
            info!("Found PCI-to-PCI bridge on bus [{}]", address.bus());
            let bridge = PciPciBridgeHeader::from_header(device, self.config_space()).unwrap();
            self.scan_bus(PciAddress::new(
                0x8000,
                bridge.secondary_bus_number(self.config_space()),
                0,
                0,
            ));
        } else {
            info!(
                "Found PCI device [0x{:0>4x}:0x{:0>4x}] on bus [{}]",
                id.0,
                id.1,
                address.bus()
            );
            self.devices.push(RwLock::new(
                EndpointHeader::from_header(device, self.config_space()).unwrap(),
            ));
        }
    }

    pub fn dump_devices(&self) {
        info!("alle gefundenen devices sind");
        for endpoint_header in &self.devices {
            let (subsystemid, subsystem_vendor_id) =
                endpoint_header.read().subsystem(&self.config_space);
            info!(
                "finde endpoint: subsystem is {:?}, subsystem_vendor_id is {:?}",
                subsystemid, subsystem_vendor_id
            );
        }
    }

    pub fn dump_devices_status_registers(&self) {
        info!("alle gefundenen devices sind");
        for endpoint_header in &self.devices {
            let status = endpoint_header.read().status(&self.config_space);
            info!("finde endpoint: status is {:?}", status);
        }
    }

    pub fn dump_devices_command_registers(&self) {
        info!("alle gefundenen devices sind");
        for endpoint_header in &self.devices {
            let command = endpoint_header.read().command(&self.config_space);
            info!("finde endpoint: command is {:?}", command);
        }
    }
}
