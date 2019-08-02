use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use selfe_sys::*;

use crate::arch;
use crate::cap::{
    memory_kind, ASIDControl, ASIDPool, CNodeRole, CNodeSlot, Cap, LocalCap, Untyped,
};
use crate::error::{ErrorExt, SeL4Error};

impl<FreePools: Unsigned> LocalCap<ASIDControl<FreePools>> {
    pub(crate) fn make_asid_pool_without_consuming_control_pool<DestRole: CNodeRole>(
        &mut self,
        ut12: LocalCap<Untyped<U12, memory_kind::General>>,
        dest_slot: CNodeSlot<DestRole>,
    ) -> Result<LocalCap<ASIDPool<arch::ASIDPoolSize>>, SeL4Error>
    where
        FreePools: Sub<U1>,
        op!(FreePools - U1): Unsigned,
    {
        let dest = dest_slot.elim().cptr;
        unsafe {
            seL4_ARM_ASIDControl_MakePool(
                self.cptr,          // _service
                ut12.cptr,          // untyped
                dest.cnode.into(),  // root
                dest.index.into(),  // index
                arch::WordSize::U8, // depth
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::new(selfe_wrap::error::APIMethod::ASIDControlMakePool, e))?;
        Ok(Cap {
            cptr: dest.index.into(),
            cap_data: ASIDPool {
                id: (arch::ASIDPoolCount::USIZE - FreePools::USIZE),
                next_free_slot: 0,
                _free_slots: PhantomData,
            },
            _role: PhantomData,
        })
    }
}
