use core::marker::PhantomData;
use core::ops::{Add, Sub};

use selfe_sys::*;

use typenum::operator_aliases::Diff;
use typenum::*;

use crate::cap::{role, CNodeRole, Cap, CapType, ChildCap, LocalCap};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;

/// There will only ever be one CNode in a process with Role = Root. The
/// cptrs any regular Cap are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<Role: CNodeRole> {
    pub(crate) radix: u8,
    pub(crate) _role: PhantomData<Role>,
}

pub type LocalCNode = CNode<role::Local>;
pub type ChildCNode = CNode<role::Child>;

#[derive(Debug)]
pub struct CNodeSlotsData<Size: Unsigned, Role: CNodeRole> {
    pub(crate) offset: usize,
    pub(crate) _size: PhantomData<Size>,
    pub(crate) _role: PhantomData<Role>,
}

/// Can only represent local CNode slots with capacity tracked at runtime
#[derive(Debug)]
pub struct WCNodeSlotsData {
    pub(crate) offset: usize,
    pub(crate) size: usize,
}

impl<Role: CNodeRole> CapType for CNode<Role> {}

impl<Size: Unsigned, Role: CNodeRole> CapType for CNodeSlotsData<Size, Role> {}

pub type CNodeSlots<Size, Role> = LocalCap<CNodeSlotsData<Size, Role>>;
pub type LocalCNodeSlots<Size> = CNodeSlots<Size, role::Local>;
pub type ChildCNodeSlots<Size> = CNodeSlots<Size, role::Child>;

pub type CNodeSlot<Role> = CNodeSlots<U1, Role>;
pub type LocalCNodeSlot = CNodeSlot<role::Local>;
pub type ChildCNodeSlot = CNodeSlot<role::Child>;

impl CapType for WCNodeSlotsData {}
pub type WCNodeSlots = LocalCap<WCNodeSlotsData>;

impl<Size: Unsigned, CapRole: CNodeRole, Role: CNodeRole> Cap<CNodeSlotsData<Size, Role>, CapRole> {
    /// A private constructor
    pub(crate) fn internal_new(
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
    pub fn with_temporary<E, F>(&mut self, f: F) -> Result<Result<(), E>, SeL4Error>
    where
        F: FnOnce(Self) -> Result<(), E>,
    {
        // Call the function with an alias/copy of self
        let r = f(Cap::internal_new(self.cptr, self.cap_data.offset));
        unsafe { self.revoke_in_reverse() }
        Ok(r)
    }

    /// Blindly attempt to revoke and delete the contents of the slots,
    /// (in reverse order) ignoring errors related to empty slots.
    pub(crate) unsafe fn revoke_in_reverse(&self) {
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

    /// weaken erases the state-tracking types on a set of CNode
    /// slots.
    pub fn weaken(self) -> WCNodeSlots {
        WCNodeSlots {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: WCNodeSlotsData {
                offset: self.cap_data.offset,
                size: Size::USIZE,
            },
        }
    }
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

        unsafe {
            seL4_CNode_Copy(
                dest_cptr,            // _service
                dest_offset,          // index
                seL4_WordBits as u8,  // depth
                parent_cnode.cptr,    // src_root
                self.cptr,            // src_index
                seL4_WordBits as u8,  // src_depth
                CapRights::RW.into(), // rights
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::CNodeCopy(e))?;
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

#[derive(Debug)]
pub enum CNodeSlotsError {
    NotEnoughSlots,
}

impl WCNodeSlots {
    /// Split the WCNodeSlots(original_size) into (WCNodeSlots(count), WCNodeSlots(original_size - count))
    /// Peel off a single cptr from these slots. Advance the state.
    pub(crate) fn alloc(&mut self, count: usize) -> Result<WCNodeSlots, CNodeSlotsError> {
        if count > self.cap_data.size {
            return Err(CNodeSlotsError::NotEnoughSlots);
        }
        let offset = self.cap_data.offset;
        self.cap_data.offset += count;
        self.cap_data.size -= count;
        Ok(Cap {
            cptr: self.cptr,
            cap_data: WCNodeSlotsData {
                offset: offset,
                size: count,
            },
            _role: PhantomData,
        })
    }

    pub(crate) fn into_strong_iter(self) -> impl Iterator<Item = LocalCNodeSlot> {
        (0..self.cap_data.size).map(move |n| Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset: self.cap_data.offset + n,
                _size: PhantomData,
                _role: PhantomData,
            },
        })
    }
}