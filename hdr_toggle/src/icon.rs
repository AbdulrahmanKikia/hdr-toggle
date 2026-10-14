//! Tray icon generation: a filled circle drawn into a 32x32 RGBA buffer.

use anyhow::{Context, Result};
use tray_icon::Icon;

const SIZE: u32 = 32;
const RADIUS: f32 = 14.0;
const GREEN: [u8; 3] = [0x2E, 0xC2, 0x4F];
const GRAY: [u8; 3] = [0x80, 0x80, 0x80];

/// Builds the tray icon: green when HDR is on, gray when off.
pub fn make_icon(on: bool) -> Result<Icon> {
    let color = if on { GREEN } else { GRAY };
    Icon::from_rgba(circle_rgba(color), SIZE, SIZE).context("failed to create tray icon")
}

/// Draws a filled circle on a transparent background with a 1px soft edge.
fn circle_rgba(color: [u8; 3]) -> Vec<u8> {
    let center = (SIZE as f32 - 1.0) / 2.0;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dist = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let coverage = (RADIUS + 0.5 - dist).clamp(0.0, 1.0);
            rgba.extend_from_slice(&color);
            rgba.push((coverage * 255.0).round() as u8);
        }
    }
    rgba
}
