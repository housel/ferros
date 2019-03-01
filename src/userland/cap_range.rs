use core::marker::PhantomData;
use typenum::{Unsigned, B1, U12, U2, U3};

use crate::userland::{
    memory_kind, role, CNode, CNodeRole, Cap, CapType, ChildCNode, ChildCap, DirectRetype,
    LocalCap, MemoryKind, PhantomCap, SeL4Error, UnmappedPage, Untyped,
};

pub struct CapRange<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> {
    pub(crate) start_cptr: usize,
    pub(crate) _cap_type: PhantomData<CT>,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _slots: PhantomData<Slots>,
}

impl<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> CapRange<CT, Role, Slots> {
    pub fn iter(self) -> impl Iterator<Item = Cap<CT, Role>> {
        (0..Slots::USIZE).map(move |offset| Cap {
            cptr: self.start_cptr + offset,
            _role: PhantomData,
            cap_data: PhantomCap::phantom_instance(),
        })
    }
}