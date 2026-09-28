// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pixel-exact regression guard: every rasterizer path (scalar and NEON) must keep
//! producing these frames byte for byte.
//!
//! The rasterizer is deterministic. egui is almost deterministic: its font-atlas discs and
//! shadow easing call `powf`, which comes from the platform's C math library. glibc and musl
//! agree on these scenes. If a hash fails on one operating system only, check egui's math
//! before blaming the rasterizer.

#[allow(
    dead_code,
    reason = "shared with the frame benchmark, which uses the rest"
)]
#[path = "../benches/scenes/mod.rs"]
mod scenes;

use scenes::{Scene, SceneState, render_frame, settled_renderer, surface_hash};

fn expected_hash(scene: Scene) -> u64 {
    match scene {
        Scene::Form => 0xaef8_18bc_3738_fee7,
        Scene::Preview => 0x9a8d_1786_69fb_7502,
        Scene::Window => 0xcb80_d9fe_542f_9c3c,
    }
}

#[test]
fn scenes_match_golden_hashes() {
    let mut mismatches = Vec::new();
    for scene in Scene::ALL {
        let (renderer, _, _) = settled_renderer(scene);
        let hash = surface_hash(&renderer);
        if hash != expected_hash(scene) {
            mismatches.push(format!("{} hash=0x{hash:016x}", scene.name()));
        }
    }
    assert!(mismatches.is_empty(), "golden mismatches: {mismatches:?}");
}

#[test]
fn skipping_unchanged_frames_matches_full_rendering() {
    let schedule = [
        Scene::Form,
        Scene::Form,
        Scene::Window,
        Scene::Window,
        Scene::Window,
        Scene::Preview,
        Scene::Preview,
        Scene::Form,
    ];
    let (skip_context, full_context) = (egui::Context::default(), egui::Context::default());
    let mut skipping = dream_soft_render::SoftwareRenderer::default();
    let mut full = dream_soft_render::SoftwareRenderer::default();
    full.set_skip_unchanged_frames(false);
    let (mut skip_state, mut full_state) = (SceneState::default(), SceneState::default());
    let mut skipped = 0;
    for (frame, scene) in schedule.iter().cycle().take(120).enumerate() {
        let outcome = render_frame(&mut skipping, &skip_context, *scene, &mut skip_state, false);
        render_frame(&mut full, &full_context, *scene, &mut full_state, false);
        skipped += usize::from(!outcome.surface_changed);
        assert_eq!(
            skipping.surface().pixels,
            full.surface().pixels,
            "frame {frame} ({})",
            scene.name()
        );
    }
    assert!(skipped > 10, "only {skipped} frames skipped");
}
