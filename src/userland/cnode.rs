use crate::userland::{role, CNodeRole, Cap, CapRights, ChildCap, LocalCap, SeL4Error};
use core::marker::PhantomData;
use core::ops::{Add, Sub};
use selfe_sys::*;
use typenum::operator_aliases::Diff;
use typenum::*;

/// There will only ever be one CNode in a process with Role = Root. The
/// cptrs any regular Cap are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<Role: CNodeRole> {
    pub(super) radix: u8,
    pub(super) _role: PhantomData<Role>,
}

pub type LocalCNode = CNode<role::Local>;
pub type ChildCNode = CNode<role::Child>;

#[derive(Debug)]
pub struct CNodeSlotsData<Size: Unsigned, Role: CNodeRole> {
    offset: usize,
    _size: PhantomData<Size>,
    _role: PhantomData<Role>,
}

pub type CNodeSlots<Size, Role> = LocalCap<CNodeSlotsData<Size, Role>>;
pub type LocalCNodeSlots<Size> = CNodeSlots<Size, role::Local>;
pub type ChildCNodeSlots<Size> = CNodeSlots<Size, role::Child>;

pub type CNodeSlot<Role> = CNodeSlots<U1, Role>;
pub type LocalCNodeSlot = CNodeSlot<role::Local>;
pub type ChildCNodeSlot = CNodeSlot<role::Child>;

impl<Size: Unsigned, CapRole: CNodeRole, Role: CNodeRole> Cap<CNodeSlotsData<Size, Role>, CapRole> {
    /// A private constructor
    pub(super) fn internal_new(
        cptr: usize,
        offset: usize,
    ) -> Cap<CNodeSlotsData<Size, Role>, CapRole> {
        Cap {
            cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset,
                _size: PhantomData,
                _role: PhantomData,
            },
        }
    }
}

impl<Size: Unsigned, Role: CNodeRole> CNodeSlots<Size, Role> {
    pub fn elim(self) -> (usize, usize, usize) {
        (self.cptr, self.cap_data.offset, Size::USIZE)
    }

    pub fn alloc<Count: Unsigned>(
        self,
    ) -> (CNodeSlots<Count, Role>, CNodeSlots<Diff<Size, Count>, Role>)
    where
        Size: Sub<Count>,
        Diff<Size, Count>: Unsigned,
    {
        let (cptr, offset, _) = self.elim();

        (
            CNodeSlots::internal_new(cptr, offset),
            CNodeSlots::internal_new(cptr, offset + Count::USIZE),
        )
    }

    pub fn iter(self) -> impl Iterator<Item = CNodeSlot<Role>> {
        let cptr = self.cptr;
        let offset = self.cap_data.offset;
        (0..Size::USIZE).map(move |n| Cap {
            cptr: cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset: offset + n,
                _size: PhantomData,
                _role: PhantomData,
            },
        })
    }
}

impl<Size: Unsigned> LocalCNodeSlots<Size> {
    /// Gain temporary access to some slots for use in a function context.
    /// When the passed function call is complete, all capabilities
    /// in this range will be revoked and deleted.
    pub unsafe fn with_temporary<E, F>(self, f: F) -> Result<(Result<(), E>, Self), SeL4Error>
    where
        F: FnOnce(Self) -> Result<(), E>,
    {
        // Call the function with an alias/copy of self
        let r = f(Cap::internal_new(self.cptr, self.cap_data.offset));
        self.revoke_in_reverse();
        Ok((r, self))
    }

    /// Blindly attempt to revoke and delete the contents of the slots,
    /// (in reverse order) ignoring errors related to empty slots.
    unsafe fn revoke_in_reverse(&self) {
        for offset in (self.cap_data.offset..self.cap_data.offset + Size::USIZE).rev() {
            // Clean up any child/derived capabilities that may have been created.
            let _err = seL4_CNode_Revoke(
                self.cptr,           // _service
                offset,              // index
                seL4_WordBits as u8, // depth
            );

            // Clean out the slot itself
            let _err = seL4_CNode_Delete(
                self.cptr,           // _service
                offset,              // index
                seL4_WordBits as u8, // depth
            );
        }
    }
}

/// Gain temporary access to some slots and memory for use in a function context.
/// When the passed function call is complete, all capabilities
/// in this range will be revoked and deleted and the memory reclaimed.
pub unsafe fn with_temporary_resources<SlotCount: Unsigned, BitSize: Unsigned, E, F>(
    slots: LocalCNodeSlots<SlotCount>,
    untyped: LocalCap<crate::userland::cap::Untyped<BitSize>>,
    f: F,
) -> Result<
    (
        Result<(), E>,
        LocalCNodeSlots<SlotCount>,
        LocalCap<crate::userland::cap::Untyped<BitSize>>,
    ),
    SeL4Error,
>
where
    F: FnOnce(
        LocalCNodeSlots<SlotCount>,
        LocalCap<crate::userland::cap::Untyped<BitSize>>,
    ) -> Result<(), E>,
{
    // Call the function with an alias/copy of self
    let r = f(
        Cap::internal_new(slots.cptr, slots.cap_data.offset),
        Cap {
            cptr: untyped.cptr,
            cap_data: crate::userland::cap::Untyped {
                _bit_size: PhantomData,
                _kind: PhantomData,
            },
            _role: PhantomData,
        },
    );
    slots.revoke_in_reverse();

    // Clean up any child/derived capabilities that may have been created from the memory
    // Because the slots and the untyped are both Local, the slots' parent CNode capability pointer
    // must be the same as the untyped's parent CNode
    let err = seL4_CNode_Revoke(
        slots.cptr,          // _service
        untyped.cptr,        // index
        seL4_WordBits as u8, // depth
    );
    if err != 0 {
        return Err(SeL4Error::CNodeRevoke(err));
    }
    Ok((r, slots, untyped))
}

impl LocalCap<ChildCNode> {
    pub fn generate_self_reference<SlotsForChild: Unsigned>(
        &self,
        parent_cnode: &LocalCap<LocalCNode>,
        dest_slots: LocalCap<CNodeSlotsData<op! {SlotsForChild + U1}, role::Child>>,
    ) -> Result<
        (
            ChildCap<ChildCNode>,
            ChildCap<CNodeSlotsData<SlotsForChild, role::Child>>,
        ),
        SeL4Error,
    >
    where
        SlotsForChild: Add<U1>,
        op! {SlotsForChild +  U1}: Unsigned,
    {
        let (dest_cptr, dest_offset, _) = dest_slots.elim();

        let err = unsafe {
            seL4_CNode_Copy(
                dest_cptr,            // _service
                dest_offset,          // index
                seL4_WordBits as u8,  // depth
                parent_cnode.cptr,    // src_root
                self.cptr,            // src_index
                seL4_WordBits as u8,  // src_depth
                CapRights::RW.into(), // rights
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeCopy(err))
        } else {
            Ok((
                Cap {
                    cptr: dest_offset,
                    _role: PhantomData,
                    cap_data: CNode {
                        radix: self.cap_data.radix,
                        _role: PhantomData,
                    },
                },
                Cap::internal_new(dest_offset, dest_offset + 1),
            ))
        }
    }
}
