use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};
use crate::error::SeL4Error;
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::super::{PageDirIndexBits, PageIndexBits, PageTableIndexBits, PageUpperDirIndexBits};
use super::PageDirectory;

const UD_MASK: usize = (((1 << PageUpperDirIndexBits::USIZE) - 1)
    << PageIndexBits::USIZE + PageTableIndexBits::USIZE + PageDirIndexBits::USIZE);

#[derive(Debug)]
pub struct PageUpperDirectory {}

impl Maps<PageDirectory> for PageUpperDirectory {
    fn map_granule<RootG, Root>(
        &mut self,
        dir: &LocalCap<PageDirectory>,
        addr: usize,
        root: &mut LocalCap<Root>,
        _rights: CapRights,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
        RootG: CapType,
    {
        match unsafe {
            seL4_ARM_PageDirectory_Map(
                dir.cptr,
                addr & UD_MASK,
                root.cptr,
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
            )
        } {
            0 => Ok(()),
            6 => Err(MappingError::Overflow),
            e => Err(MappingError::IntermediateLayerFailure(
                SeL4Error::PageDirectoryMap(e),
            )),
        }
    }
}

impl CapType for PageUpperDirectory {}

impl PhantomCap for PageUpperDirectory {
    fn phantom_instance() -> Self {
        PageUpperDirectory {}
    }
}

impl DirectRetype for PageUpperDirectory {
    type SizeBits = super::super::PageUpperDirBits;
    fn sel4_type_id() -> usize {
        _mode_object_seL4_ARM_PageUpperDirectoryObject as usize
    }
}
