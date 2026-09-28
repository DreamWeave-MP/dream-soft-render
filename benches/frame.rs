// SPDX-License-Identifier: MIT OR Apache-2.0

//! Frame benchmark over realistic egui scenes.
//!
//! `cargo bench --bench frame` renders each scene a few hundred times and reports rasterize
//! and whole-frame times in microseconds. Options:
//!
//! - `--frames N`: measured frames per scene.
//! - `--scene NAME`: run only `form`, `preview`, or `window`.
//! - `--no-feathering`: turn off egui's shape anti-aliasing, as Dream-INI's `PortMaster`
//!   build does.
//! - `--hash`: print each scene's golden hash.
//! - `--dump DIR`: write each scene's surface to `DIR/<scene>.rgba`.
//! - `--stats`: print the renderer's deep-stat log lines for one frame.
//! - `--settle N`: shorten the warm-up, for instruction tracing under qemu.
//!
//! Without `--bench` (the way `cargo test --all-targets` runs it) each scene renders once.

#[allow(dead_code, reason = "shared with the golden tests, which use the rest")]
mod scenes;

use std::path::PathBuf;
use std::time::Instant;

use scenes::{
    SETTLE_FRAMES, Scene, render_frame, settled_renderer_after, stats_lines, surface_hash,
};

fn main() {
    let mut frames = 400_usize;
    let mut dump = None;
    let mut print_hash = false;
    let mut print_stats = false;
    let mut only = None;
    let mut settle_frames = SETTLE_FRAMES;
    let mut feathering = true;
    let mut bench_mode = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bench" => bench_mode = true,
            "--frames" => {
                frames = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--frames N");
            }
            "--dump" => dump = Some(PathBuf::from(args.next().expect("--dump DIR"))),
            "--hash" => print_hash = true,
            "--stats" => print_stats = true,
            "--scene" => only = args.next(),
            "--no-feathering" => feathering = false,
            "--settle" => {
                settle_frames = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--settle N");
            }
            _ => {}
        }
    }
    if !bench_mode && dump.is_none() && !print_hash && !print_stats {
        frames = 1;
    }

    for scene in Scene::ALL {
        if only.as_deref().is_some_and(|name| name != scene.name()) {
            continue;
        }
        let (mut renderer, context, mut state) =
            settled_renderer_after(scene, settle_frames, feathering);
        let mut rasterize = Vec::with_capacity(frames);
        let mut total = Vec::with_capacity(frames);
        for _ in 0..frames {
            let start = Instant::now();
            let outcome = render_frame(&mut renderer, &context, scene, &mut state, true);
            total.push(start.elapsed().as_micros());
            rasterize.push(outcome.timings.map_or(0, |timings| timings.rasterize));
        }
        let (r50, r10, r90) = percentiles(&mut rasterize);
        let (t50, _, _) = percentiles(&mut total);
        println!(
            "{:<8} rasterize_us p50={r50:>7} p10={r10:>7} p90={r90:>7}   render_us p50={t50:>7}   primitives={}",
            scene.name(),
            render_frame(&mut renderer, &context, scene, &mut state, false).primitive_count,
        );
        if print_stats {
            for line in stats_lines(&mut renderer, &context, scene, &mut state) {
                println!("{} {line}", scene.name());
            }
        }
        if print_hash {
            println!(
                "{:<8} hash=0x{:016x}",
                scene.name(),
                surface_hash(&renderer)
            );
        }
        if let Some(dir) = &dump {
            std::fs::create_dir_all(dir).expect("create dump dir");
            std::fs::write(
                dir.join(format!("{}.rgba", scene.name())),
                &renderer.surface().pixels,
            )
            .expect("write dump");
        }
    }
}

/// p50, p10, and p90 of the samples, in microseconds.
fn percentiles(samples: &mut [u128]) -> (u128, u128, u128) {
    samples.sort_unstable();
    let at = |tenths: usize| samples[(samples.len() - 1) * tenths / 10];
    (at(5), at(1), at(9))
}
