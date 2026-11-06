use std::fs;

use slot_power::{cpu_step, CpuCap};

const FLIP_STEPS: [u32; 6] = [408_000, 600_000, 816_000, 1_104_000, 1_416_000, 1_800_000];

#[test]
fn the_cap_takes_the_highest_step_at_or_under_it() {
    assert_eq!(cpu_step(&FLIP_STEPS, 1_200_000), 1_104_000);
    assert_eq!(cpu_step(&FLIP_STEPS, 1_800_000), 1_800_000);
    assert_eq!(cpu_step(&FLIP_STEPS, 1_416_000), 1_416_000);
}

#[test]
fn a_cap_under_every_step_takes_the_lowest_and_unlisted_steps_take_the_cap() {
    assert_eq!(cpu_step(&FLIP_STEPS, 100_000), 408_000);
    assert_eq!(cpu_step(&[], 1_200_000), 1_200_000);
}

#[test]
fn only_the_top_clock_is_written_and_only_when_it_changes() {
    let d = tempfile::tempdir().unwrap();
    let policy = d.path();
    fs::write(policy.join("scaling_max_freq"), "1800000\n").unwrap();
    fs::write(policy.join("scaling_governor"), "ondemand\n").unwrap();
    fs::write(
        policy.join("scaling_available_frequencies"),
        "408000 600000 816000 1104000 1416000 1800000 \n",
    )
    .unwrap();
    let mut cpu = CpuCap::at(policy).expect("a policy with a top clock");
    cpu.cap(1_200_000);
    assert_eq!(
        fs::read_to_string(policy.join("scaling_max_freq")).unwrap(),
        "1104000"
    );
    fs::write(policy.join("scaling_max_freq"), "touched").unwrap();
    cpu.cap(1_150_000);
    assert_eq!(
        fs::read_to_string(policy.join("scaling_max_freq")).unwrap(),
        "touched",
        "the same step is not written again"
    );
    cpu.cap(1_800_000);
    assert_eq!(
        fs::read_to_string(policy.join("scaling_max_freq")).unwrap(),
        "1800000"
    );
    assert_eq!(
        fs::read_to_string(policy.join("scaling_governor")).unwrap(),
        "ondemand\n"
    );
}

#[test]
fn a_policy_without_a_top_clock_has_no_cap() {
    let d = tempfile::tempdir().unwrap();
    assert!(CpuCap::at(d.path()).is_none());
}
