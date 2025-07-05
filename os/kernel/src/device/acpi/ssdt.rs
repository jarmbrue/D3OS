use acpi::sdt::SdtHeader;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ssdt {
    header: SdtHeader,
    //definition block n bytes of aml code
}