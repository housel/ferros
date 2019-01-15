use core::marker::PhantomData;
use core::ops::Sub;
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U1, U1024};

pub trait CNodeRole: private::SealedRole {}

pub mod role {
    use super::CNodeRole;

    #[derive(Debug)]
    pub struct Local {}
    impl CNodeRole for Local {}

    #[derive(Debug)]
    pub struct Child {}
    impl CNodeRole for Child {}
}

mod private {
    pub trait SealedRole {}
    impl SealedRole for super::role::Local {}
    impl SealedRole for super::role::Child {}
}

/// There will only ever be one CNode in a process with Role = Root. The
/// cptrs any regular Cap are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<FreeSlots: Unsigned, Role: CNodeRole> {
    pub(super) radix: u8,
    pub(super) next_free_slot: usize,
    pub(super) cptr: usize,
    pub(super) _free_slots: PhantomData<FreeSlots>,
    pub(super) _role: PhantomData<Role>,
}

#[derive(Debug)]
pub(super) struct CNodeSlot {
    pub(super) cptr: usize,
    pub(super) offset: usize,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> CNode<FreeSlots, Role> {
    // TODO: reverse these args to be consistent with everything else
    pub(super) fn consume_slot(self) -> (CNode<Sub1<FreeSlots>, Role>, CNodeSlot)
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        (
            // TODO: use mem::transmute
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot + 1,
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
            CNodeSlot {
                cptr: self.cptr,
                offset: self.next_free_slot,
            },
        )
    }

    // Reserve Count slots. Return another node with the same cptr, but the
    // requested capacity.
    pub fn reserve_region<Count: Unsigned>(
        self,
    ) -> (CNode<Count, Role>, CNode<Diff<FreeSlots, Count>, Role>)
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        (
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot,
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
            // TODO: use mem::transmute
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot + Count::to_usize(),
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        )
    }

    pub fn reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = CNode<U1, Role>>,
        CNode<Diff<FreeSlots, Count>, Role>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        let iter_radix = self.radix;
        let iter_cptr = self.cptr;
        (
            (self.next_free_slot..self.next_free_slot + Count::to_usize()).map(move |slot| CNode {
                radix: iter_radix,
                next_free_slot: slot,
                cptr: iter_cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            }),
            // TODO: use mem::transmute
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot + Count::to_usize(),
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        )
    }
}

// TODO: how many slots are there really? We should be able to know this at build
// time.
// Answer: The radix is 19, and there are 12 initial caps. But there are also a bunch
// of random things in the bootinfo.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(bootinfo: &'static seL4_BootInfo) -> CNode<U1024, role::Local> {
    CNode {
        radix: 19,
        next_free_slot: 1000, // TODO: look at the bootinfo to determine the real value
        cptr: seL4_CapInitThreadCNode as usize,
        _free_slots: PhantomData,
        _role: PhantomData,
    }
}