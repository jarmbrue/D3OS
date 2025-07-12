use bitfield_struct::bitfield;
use log::info;
use x86_64::structures::paging::Page;

#[derive(Debug, PartialEq)]
pub struct RASReg {}
#[derive(Debug, PartialEq)]
pub struct SecurityReg {}
#[derive(Debug, PartialEq)]
pub struct LinkReg {
    pub address: *const u32,
}
#[derive(Debug, PartialEq)]
pub struct HDMDecorderReg {
    pub version: u8,
    pub address: *const u32,
}
#[derive(Debug, PartialEq)]
pub struct ExtendedSecurtiyReg {
    pub address: *const u32,
}
#[derive(Debug, PartialEq)]
pub struct IDEReg {}
#[derive(Debug, PartialEq)]
pub struct SnoopFilterReg {}
#[derive(Debug, PartialEq)]
pub struct TimeoutAndIsolationReg {}
#[derive(Debug, PartialEq)]
pub struct CacheMemExtendedRegisterReg {}
#[derive(Debug, PartialEq)]
pub struct BIRouteTableReg {}
#[derive(Debug, PartialEq)]
pub struct BIDecoderReg {}
#[derive(Debug, PartialEq)]
pub struct CacheIdRouteTableReg {}
#[derive(Debug, PartialEq)]
pub struct CacheIdDecoderReg {}
#[derive(Debug, PartialEq)]
pub struct ExtendedHDMDecoderReg {}
#[derive(Debug, PartialEq)]
pub struct ExtendedMetaDataReg {}

#[derive(Debug, PartialEq)]
pub enum CXLCapability {
    Null,
    CXL,
    RAS(RASReg),
    Security(SecurityReg),
    Link(LinkReg),
    HDMDecorder(HDMDecorderReg),
    ExtendedSecurtiy(ExtendedSecurtiyReg),
    IDE(IDEReg),
    SnoopFilter(SnoopFilterReg),
    TimeoutAndIsolation(TimeoutAndIsolationReg),
    CacheMemExtendedRegister(CacheMemExtendedRegisterReg),
    BIRouteTable(BIRouteTableReg),
    BIDecoder(BIDecoderReg),
    CacheIdRouteTable(CacheIdRouteTableReg),
    CacheIdDecoder(CacheIdDecoderReg),
    ExtendedHDMDecoder(ExtendedHDMDecoderReg),
    ExtendedMetaData(ExtendedMetaDataReg),
    Unknown { id: u16, version: u8 },
}

impl CXLCapability {
    fn parse(
        base: *const u8,
        header: &CXLAbstractCapabilitiesHeader,
    ) -> Option<CXLCapability> {
        let address = unsafe { base.offset(header.offset() as isize).cast() };
        match header.id() {
            0x0 => Some(Self::Null),
            0x1 => Some(Self::CXL),
            0x2 => Some(Self::RAS(RASReg {})),
            0x3 => Some(Self::Security(SecurityReg {})),
            0x4 => Some(Self::Link(LinkReg { address })),
            0x5 => Some(Self::HDMDecorder(HDMDecorderReg { address, version: header.version() })),
            0x6 => Some(Self::ExtendedSecurtiy(ExtendedSecurtiyReg { address })),
            0x7 => Some(Self::IDE(IDEReg {})),
            0x8 => Some(Self::SnoopFilter(SnoopFilterReg {})),
            0x9 => Some(Self::TimeoutAndIsolation(TimeoutAndIsolationReg {})),
            0xa => Some(Self::CacheMemExtendedRegister(
                CacheMemExtendedRegisterReg {},
            )),
            0xb => Some(Self::BIRouteTable(BIRouteTableReg {})),
            0xc => Some(Self::BIDecoder(BIDecoderReg {})),
            0xd => Some(Self::CacheIdRouteTable(CacheIdRouteTableReg {})),
            0xe => Some(Self::CacheIdDecoder(CacheIdDecoderReg {})),
            0xf => Some(Self::ExtendedHDMDecoder(ExtendedHDMDecoderReg {})),
            0x10 => Some(Self::ExtendedMetaData(ExtendedMetaDataReg {})),
            _ => Some(Self::Unknown {
                id: header.id(),
                version: header.version(),
            }),
        }
    }
}

#[bitfield(u32)]
struct CXLCapabilityListHeader {
    id: u16,
    #[bits(4)]
    version: u8,
    #[bits(4)]
    cache_mem_version: u8,
    size: u8,
}

#[bitfield(u32)]
pub struct CXLAbstractCapabilitiesHeader {
    pub id: u16,
    #[bits(4)]
    pub version: u8,
    #[bits(12)]
    pub offset: u16,
}

pub struct CXLCapabilityIterator {
    address: *const CXLAbstractCapabilitiesHeader,
    offset: u8,
    size: u8,
}

impl CXLCapabilityIterator {
    pub fn new(primary_range: &Page) -> Option<Self> {
        let header_ptr = primary_range
            .start_address()
            .as_ptr::<CXLCapabilityListHeader>();
        let header = unsafe { header_ptr.read() };
        Some(Self {
            address: header_ptr.cast(),
            offset: 1,
            size: header.size(),
        })
    }
}

impl Iterator for CXLCapabilityIterator {
    type Item = CXLCapability;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset > self.size {
            return None;
        }
        let data = unsafe { self.address.offset(self.offset as isize).read() };
        let result = CXLCapability::parse(self.address.cast(), &data);
        self.offset += 1;
        return result;
    }
}
