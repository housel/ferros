use core::marker::PhantomData;
use core::ops::Sub;
use crate::pow::Pow;
use crate::userland::{
    paging, role, AssignedPageDirectory, Cap, CapRights, LocalCap, MappedPage, MappedPageTable,
    PhantomCap, SeL4Error, UnmappedPage, UnmappedPageTable,
};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1};

// vspace related capability operations
impl<FreeSlots: Unsigned> LocalCap<AssignedPageDirectory<FreeSlots>> {
    pub fn map_page_table(
        self,
        page_table: Cap<UnmappedPageTable, role::Local>,
        virtual_address: usize,
        page_dir: LocalCap<AssignedPageDirectory<Sub1<FreeSlots>>>,
    ) -> Result<Cap<MappedPageTable<Pow<paging::PageTableBits>>, role::Local>, SeL4Error>
    where
        FreeSlots: Sub<B1>,
        Sub1<B1>: Unsigned,
    {
        // map the page table
        let err = unsafe {
            seL4_ARM_PageTable_Map(
                page_table.cptr,
                self.cptr,
                virtual_address,
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled, // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever
            )
        };

        if err != 0 {
            return Err(SeL4Error::MapPageTable(err));
        }
        Ok(
            Cap {
                cptr: page_table.cptr,
                _role: PhantomData,
                cap_data: MappedPageTable {
                    vaddr: virtual_address,
                    next_free_slot: 0,
                    _free_slots: PhantomData,
                },
            },
            // page_dir
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: AssignedPageDirectory {
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                },
            },
        )
    }

    pub fn map_page(
        &mut self,
        page: Cap<UnmappedPage, role::Local>,
        virtual_address: usize,
    ) -> Result<Cap<MappedPage, role::Local>, SeL4Error> {
        let err = unsafe {
            seL4_ARM_Page_Map(
                page.cptr,
                self.cptr,
                virtual_address,
                CapRights::RW.into(), // rights
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled
                    // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
            )
        };
        if err != 0 {
            return Err(SeL4Error::MapPage(err));
        }
        Ok(Cap {
            cptr: page.cptr,
            cap_data: MappedPage {
                vaddr: virtual_address,
            },
            _role: PhantomData,
        })
    }
}

impl<FreeSlots: Unsigned> Cap<MappedPageTable<FreeSlots>, role::Local> {
    pub fn unmap(self) -> Result<Cap<UnmappedPageTable, role::Local>, SeL4Error> {
        let err = unsafe { seL4_ARM_PageTable_Unmap(self.cptr) };
        if err != 0 {
            return Err(SeL4Error::UnmapPageTable(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}

impl Cap<MappedPage, role::Local> {
    pub fn unmap(self) -> Result<Cap<UnmappedPage, role::Local>, SeL4Error> {
        let err = unsafe { seL4_ARM_Page_Unmap(self.cptr) };
        if err != 0 {
            return Err(SeL4Error::UnmapPage(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}
