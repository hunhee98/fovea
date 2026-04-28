//! A/B benchmark: MotionTrigger with vs without global-motion correction.
//!
//! Runs four trigger configurations against a clip and reports the per-trigger
//! event count plus mean/median energy. Use to validate that
//! `with_global_motion` reduces false positives on synthetic-camera-pan inputs.
//!
//! Usage:
//!     cargo run --release --example global_motion_ab -- <path-to-mp4>

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use fovea_mv_core::global_motion::GlobalMotionEstimator;
use fovea_mv_core::triggers::MotionTrigger;
use fovea_mv_core::Trigger;
use fovea_mv_stream::FfmpegSource;

struct Run {
    name: &'static str,
    trigger: MotionTrigger,
    fires: u32,
    energies: Vec<u64>,
}

impl Run {
    fn new(name: &'static str, trigger: MotionTrigger) -> Self {
        Self { name, trigger, fires: 0, energies: Vec::new() }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: global_motion_ab <path-to-mp4>");
        return ExitCode::from(2);
    }
    let clip = PathBuf::from(&args[1]);

    let mut source = match FfmpegSource::open_file(&clip) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open failed: {e}");
            return ExitCode::from(1);
        }
    };

    // Threshold tunable via $FOVEA_BENCH_THRESHOLD (default 1000).
    // 1000 is between baseline pan energy (~4000) and corrected residual (~150),
    // which gives a clean separation on synthetic-ptz clips.
    let threshold: u64 = env::var("FOVEA_BENCH_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let window: usize = env::var("FOVEA_BENCH_WINDOW")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let mut runs = vec![
        Run::new("baseline", MotionTrigger::new(threshold)),
        Run::new(
            "global_motion",
            MotionTrigger::new(threshold).with_global_motion(GlobalMotionEstimator),
        ),
        Run::new("window", MotionTrigger::new(threshold).with_window(window)),
        Run::new(
            "global_motion+window",
            MotionTrigger::new(threshold)
                .with_global_motion(GlobalMotionEstimator)
                .with_window(window),
        ),
    ];

    let mut total_packets: u32 = 0;
    let mut total_mvs: u64 = 0;

    loop {
        let pkt = match source.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => {
                eprintln!("decode error: {e}");
                return ExitCode::from(1);
            }
        };
        total_packets += 1;
        total_mvs += pkt.mvs.len() as u64;

        for run in runs.iter_mut() {
            if let Some(ev) = run.trigger.evaluate(pkt) {
                run.fires += 1;
                run.energies.push(ev.energy);
            }
        }
    }

    println!("clip: {}", clip.display());
    println!("packets: {} | total MVs: {}", total_packets, total_mvs);
    println!();
    println!(
        "{:<24} | {:>5} | {:>10} | {:>10}",
        "config", "fires", "mean E", "median E"
    );
    println!("{}", "-".repeat(60));
    for run in &runs {
        let n = run.energies.len() as u64;
        let (mean, median) = if n == 0 {
            (0, 0)
        } else {
            let sum: u64 = run.energies.iter().sum();
            let mean = sum / n;
            let mut sorted = run.energies.clone();
            sorted.sort_unstable();
            let median = sorted[sorted.len() / 2];
            (mean, median)
        };
        println!(
            "{:<24} | {:>5} | {:>10} | {:>10}",
            run.name, run.fires, mean, median
        );
    }

    ExitCode::SUCCESS
}
