#![no_std]

extern crate iron_pegasus;
extern crate sel4_sys;
extern crate typenum;

use sel4_sys::*;

macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}

use iron_pegasus::micro_alloc::{self, GetUntyped};
use iron_pegasus::userland::{root_cnode, spawn, BootInfo};
use typenum::{U12, U20};

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

pub fn run(raw_boot_info: &'static seL4_BootInfo) {
    #[cfg(test_case = "root_task_runs")]
    {
        debug_println!("\nhello from the root task!\n");
    }

    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)
        .expect("Couldn't set up bootstrap allocator");

    // wrap bootinfo caps
    let root_cnode = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, _, _, _, root_cnode) = ut20.quarter(root_cnode).expect("quarter");
    let (ut16, child_cnode_ut, child_proc_ut, _, root_cnode) =
        ut18.quarter(root_cnode).expect("quarter");
    let (ut14, _, _, _, root_cnode) = ut16.quarter(root_cnode).expect("quarter");
    let (ut12, asid_pool_ut, _, _, root_cnode) = ut14.quarter(root_cnode).expect("quarter");
    let (ut10, _, _, _, root_cnode) = ut12.quarter(root_cnode).expect("quarter");
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode).expect("quarter");
    let (_ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    // child cnode
    let (child_cnode, root_cnode) = child_cnode_ut
        .retype_local_cnode::<_, U12>(root_cnode)
        .expect("Couldn't retype to child proc cnode");

    let params = ProcParams { value: 42 };

    let _root_cnode = spawn(
        proc_main,
        params,
        child_cnode,
        255, // priority
        child_proc_ut,
        &mut boot_info,
        root_cnode,
    )
    .expect("spawn process");

    yield_forever();
}

pub struct ProcParams {
    pub value: usize,
}

impl iron_pegasus::userland::RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

#[cfg(test_case = "root_task_runs")]
pub extern "C" fn proc_main(_params: *const ProcParams) {}

#[cfg(test_case = "process_runs")]
pub extern "C" fn proc_main(params: *const ProcParams) {
    debug_println!("\nThe value inside the process is {}\n", unsafe {
        (&*params).value
    });
}

#[cfg(test_case = "memory_read_protection")]
pub extern "C" fn proc_main(_params: *const ProcParams) {
    debug_println!("\nAttempting to cause a segmentation fault...\n");

    unsafe {
        let x: *const usize = 0x88888888usize as _;
        debug_println!("Value from arbitrary memory is: {}", *x);
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}

#[cfg(test_case = "memory_write_protection")]
pub extern "C" fn proc_main(_params: *const ProcParams) {
    debug_println!("\nAttempting to write to the code segment...\n");

    unsafe {
        let x: *mut usize = proc_main as _;
        *x = 42;
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}
