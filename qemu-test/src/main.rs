extern crate regex;
extern crate rexpect;

fn main() {}

use lazy_static::lazy_static;
use regex::Regex;
use rexpect::process::signal::Signal;
use rexpect::session::spawn_command;
use std::io::{self, Write};
use std::process::Command;
use std::sync::Mutex;

lazy_static! {
    static ref SEQUENTIAL_TEST_MUTEX: Mutex<()> = Mutex::new(());
}
macro_rules! sequential_test {
    (fn $name:ident() $body:block) => {
        #[test]
        fn $name() {
            let _guard = $crate::SEQUENTIAL_TEST_MUTEX.lock();
            {
                $body
            }
        }
    };
}

fn run_qemu_test<F>(
    test_case: &str,
    pass_line: Regex,
    fail_line: Regex,
    ready_line_and_func: Option<(Regex, F)>,
    serial_override: Option<&str>,
    supplemental_feature_flags: Option<Vec<(&'static str, &'static str)>>,
) where
    F: Fn(),
{
    let rust_identifier_regex: Regex =
        Regex::new("(^[a-zA-Z][a-zA-Z0-9_]*$)|(^_[a-zA-Z0-9_]+$)").unwrap();
    let is_rust_id = |s| rust_identifier_regex.is_match(s);
    if !is_rust_id(test_case) {
        panic!(
            "Invalid test case test_case {}. Test case name must be a valid rust identifier",
            test_case
        );
    }

    let sel4_arch = "aarch32";
    let platform = "sabre";
    let test_extra_flag_pairs = if let Some(flags) = supplemental_feature_flags {
        flags.iter().map(|(k, v)| {
            if !is_rust_id(k) || !is_rust_id(v) {
                panic!("Invalid extra test feature flag passed: ({}, {}). Extra flags must be valid rust identifiers", k, v)
            }
            format!("{}=\"{}\"", k, v)
        })
        .collect::<Vec<_>>()
        .join(",")
    } else {
        "".to_string()
    };
    let escaped_test_extra_flag_pairs = test_extra_flag_pairs.replace(r#"""#, r#"\""#);

    let mut build_command = Command::new("selfe");
    (&mut build_command)
        .arg("build")
        .arg("--sel4_arch")
        .arg(sel4_arch)
        .arg("--platform")
        .arg(platform)
        .arg("-v")
        .current_dir("test-project")
        .env("TEST_CASE", test_case)
        .env("TEST_EXTRA_FLAG_PAIRS", test_extra_flag_pairs.clone());

    println!(
        r#"running: TEST_CASE={} TEST_EXTRA_FLAG_PAIRS="{}" {:?}"#,
        test_case, escaped_test_extra_flag_pairs, build_command
    );
    let build_result = build_command.output().expect("Couldn't run `selfe build`");
    if !build_result.status.success() {
        io::stdout().write_all(&build_result.stdout).unwrap();
        io::stderr().write_all(&build_result.stderr).unwrap();
    }
    assert!(build_result.status.success());

    let mut sim_command = Command::new("selfe");
    sim_command.arg("simulate");

    if let Some(opt) = serial_override {
        sim_command.arg("--serial-override").arg(opt);
    }

    sim_command
        .arg("--sel4_arch")
        .arg(sel4_arch)
        .arg("--platform")
        .arg(platform)
        .arg("-v")
        .current_dir("test-project")
        .env("TEST_CASE", test_case)
        .env("TEST_EXTRA_FLAG_PAIRS", test_extra_flag_pairs);

    println!(
        r#"running: TEST_CASE={} TEST_EXTRA_FLAG_PAIRS="{}" {:?}"#,
        test_case, escaped_test_extra_flag_pairs, sim_command
    );

    let mut sim = spawn_command(sim_command, Some(10000)).expect("Couldn't start simulate command");

    match ready_line_and_func {
        Some((rl, rl_func)) => {
            let mut ready_fired = false;

            loop {
                let line = sim
                    .read_line()
                    .expect("couldn't read line from simulate process");
                println!("{}", line);

                if !ready_fired && rl.is_match(&line) {
                    rl_func();
                    ready_fired = true;
                }

                if pass_line.is_match(&line) {
                    sim.process.kill(Signal::SIGKILL).unwrap();
                    break;
                }

                if fail_line.is_match(&line) {
                    sim.process.kill(Signal::SIGKILL).unwrap();
                    panic!("Output line matched failure pattern: {}", line);
                }
            }
        }
        None => loop {
            let line = sim
                .read_line()
                .expect("couldn't read line from simulate process");
            println!("{}", line);

            if pass_line.is_match(&line) {
                sim.process.kill(Signal::SIGKILL).unwrap();
                break;
            }

            if fail_line.is_match(&line) {
                sim.process.kill(Signal::SIGKILL).unwrap();
                panic!("Output line matched failure pattern: {}", line);
            }
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    sequential_test! {
        fn test_root_task_runs() {
            run_qemu_test::<fn()>(
                "root_task_runs",
                Regex::new(".*hello from the root task.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn test_process_runs() {
            run_qemu_test::<fn()>(
                "process_runs",
                Regex::new(".*The value inside the process is 42.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn memory_read_protection() {
            run_qemu_test::<fn()>(
                "memory_read_protection",
                Regex::new(".*vm fault on data.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn memory_write_protection() {
            run_qemu_test::<fn()>(
                "memory_write_protection",
                Regex::new(".*vm fault on data.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn child_process_cap_management() {
            run_qemu_test::<fn()>(
                "child_process_cap_management",
                Regex::new(".*Split, retyped, and deleted caps in a child process.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("single_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn over_register_size_params() {
            run_qemu_test::<fn()>(
                "over_register_size_params",

                Regex::new(".*The child process saw a first value of bbbbbbbb, a mid value of aaaaaaaa, and a last value of cccccccc.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("single_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn call_and_response_loop() {
            run_qemu_test::<fn()>(
                "call_and_response_loop",

                Regex::new(".*Call and response addition finished.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("dual_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn shared_page_queue() {
            run_qemu_test::<fn()>(
                "shared_page_queue",
                Regex::new(".*done producing!.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                Some(vec![("dual_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn fault_pair() {
            run_qemu_test::<fn()>(
                "fault_pair",

                Regex::new(".*Caught a fault: CapFault\\(CapFault \\{ sender: Badge \\{ inner: 0 \\}, in_receive_phase: false, cap_address: 314159 \\}\\).*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                None,
            );
        }
    }

    sequential_test! {
        fn double_door_backpressure() {
            run_qemu_test::<fn()>(
                "double_door_backpressure",
                Regex::new(".*Final state: State \\{ interrupt_count: 1, queue_e_element_count: 20, queue_e_sum: 190, queue_f_element_count: 20, queue_f_sum: 190 \\}.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                None,
            );
        }
    }

    sequential_test! {
        fn uart() {
            use std::net::TcpStream;
            use std::io::Write;

            run_qemu_test(
                "uart",
                Regex::new(".*got byte: 1.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some((Regex::new(".*thou art ready.*").unwrap(),
                || {
                    let mut stream = TcpStream::connect("localhost:8888").expect("connect stream");
                    stream.write(&[1]).expect("write stream");
                })),
                Some("-serial tcp:localhost:8888,server,nowait,nodelay -serial mon:stdio"),
                None,
            );
        }
    }

    sequential_test! {
        fn dont_tread_on_me() {
            run_qemu_test::<fn()>(
                "dont_tread_on_me",
                Regex::new(".*not changed at all.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                None,
            );
        }
    }
}
