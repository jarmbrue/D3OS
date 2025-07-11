use bitfield_struct::bitfield;
use log::info;
use core::slice;
use x86_64::structures::paging::Page;

#[bitfield(u16)]
pub struct CXLCapabilityListHeader {
    #[bits(4)]
    version: u8,
    #[bits(4)]
    cache_mem_version: u8,
    size: u8,
}

#[bitfield(u16)]
struct VersionAndOffset {
    #[bits(4)]
    pub version: u8,
    #[bits(12)]
    pub offset: u16,
}

#[derive(Debug)]
pub struct RASReg {
    version_and_offset: VersionAndOffset,
}
#[derive(Debug)]
pub struct SecurityReg {}
#[derive(Debug)]
pub struct LinkReg {}
#[derive(Debug)]
pub struct HDMDecorderReg {}
#[derive(Debug)]
pub struct ExtendedSecurtiyReg {}
#[derive(Debug)]
pub struct IDEReg {}
#[derive(Debug)]
pub struct SnoopFilterReg {}
#[derive(Debug)]
pub struct TimeoutAndIsolationReg {}
#[derive(Debug)]
pub struct CacheMemExtendedRegisterReg {}
#[derive(Debug)]
pub struct BIRouteTableReg {}
#[derive(Debug)]
pub struct BIDecoderReg {}
#[derive(Debug)]
pub struct CacheIdRouteTableReg {}
#[derive(Debug)]
pub struct CacheIdDecoderReg {}
#[derive(Debug)]
pub struct ExtendedHDMDecoderReg {}
#[derive(Debug)]
pub struct ExtendedMetaDataReg {}

#[derive(Debug)]
#[repr(C, u16)]
pub enum CXLCapability {
    Null,
    CXL(CXLCapabilityListHeader),
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
}

pub fn get_capabilities(primary_range: &Page) -> Option<&'static [CXLCapability]> {
    let header_ptr = primary_range.start_address().as_ptr::<CXLCapability>();
    info!("reading Capabilities from 0x{:x}", header_ptr as usize);
    let header = unsafe { header_ptr.read() };
    info!("CXLCapabilityListHeader: {:?}", header);
    if let CXLCapability::CXL(list_header) = header {
        Some(unsafe { slice::from_raw_parts(header_ptr.offset(1), list_header.size() as usize) })
    } else {
        None
    }
}

