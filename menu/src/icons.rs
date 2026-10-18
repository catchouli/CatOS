//! Icons drawn as vector shapes, so the menu needs no image files.
//! Coordinates are fractions of the icon's size, (0,0) top-left.

use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, vec2};
use std::f32::consts::PI;

pub fn draw(p: &Painter, name: &str, center: Pos2, size: f32, color: Color32, cutout: Color32) {
    let origin = center - vec2(size, size) / 2.0;
    let at = |x: f32, y: f32| origin + vec2(x, y) * size;
    let rect = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), vec2(w, h) * size);
    let stroke = Stroke::new((size / 14.0).max(1.5), color);
    let r = |f: f32| CornerRadius::same((size * f) as u8);
    let arc = |cx: f32, cy: f32, rad: f32, from: f32, to: f32| {
        let pts = (0..=32)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / 32.0).to_radians();
                at(cx + rad * a.cos(), cy - rad * a.sin())
            })
            .collect::<Vec<_>>();
        Shape::line(pts, stroke)
    };
    let tri = |pts: [(f32, f32); 3], fill: Color32| {
        Shape::convex_polygon(pts.iter().map(|&(x, y)| at(x, y)).collect(), fill, Stroke::NONE)
    };

    match name {
        "tv" => {
            p.rect_stroke(rect(0.08, 0.16, 0.84, 0.56), r(0.08), stroke, StrokeKind::Middle);
            p.line_segment([at(0.35, 0.86), at(0.65, 0.86)], stroke);
            p.line_segment([at(0.5, 0.72), at(0.5, 0.86)], stroke);
        }
        "gear" => {
            p.circle_stroke(at(0.5, 0.5), size * 0.27, stroke);
            p.circle_stroke(at(0.5, 0.5), size * 0.1, stroke);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                let (s, c) = a.sin_cos();
                p.line_segment(
                    [at(0.5 + 0.3 * c, 0.5 + 0.3 * s), at(0.5 + 0.44 * c, 0.5 + 0.44 * s)],
                    Stroke::new(stroke.width * 1.8, color),
                );
            }
        }
        "play" => {
            p.circle_stroke(at(0.5, 0.5), size * 0.44, stroke);
            p.add(tri([(0.41, 0.3), (0.41, 0.7), (0.72, 0.5)], color));
        }
        "youtube" => {
            p.rect_filled(rect(0.05, 0.2, 0.9, 0.6), r(0.16), color);
            p.add(tri([(0.41, 0.35), (0.41, 0.65), (0.66, 0.5)], cutout));
        }
        "film" => {
            p.rect_stroke(rect(0.12, 0.08, 0.76, 0.84), r(0.07), stroke, StrokeKind::Middle);
            p.line_segment([at(0.28, 0.08), at(0.28, 0.92)], stroke);
            p.line_segment([at(0.72, 0.08), at(0.72, 0.92)], stroke);
            for y in [0.2, 0.4, 0.6, 0.8] {
                p.rect_filled(rect(0.16, y - 0.035, 0.08, 0.07), 0.0, color);
                p.rect_filled(rect(0.76, y - 0.035, 0.08, 0.07), 0.0, color);
            }
            p.add(tri([(0.42, 0.38), (0.42, 0.62), (0.6, 0.5)], color));
        }
        "power" => {
            p.add(arc(0.5, 0.54, 0.38, 130.0, 410.0));
            p.line_segment([at(0.5, 0.06), at(0.5, 0.48)], stroke);
        }
        "restart" => {
            p.add(arc(0.5, 0.5, 0.38, 30.0, 300.0));
            p.add(tri([(0.68, 0.16), (0.95, 0.24), (0.76, 0.44)], color));
        }
        "network" => {
            for rad in [0.42, 0.28] {
                p.add(arc(0.5, 0.78, rad, 45.0, 135.0));
            }
            p.circle_filled(at(0.5, 0.74), size * 0.07, color);
        }
        "sliders" => {
            for (x, knob) in [(0.24, 0.62), (0.5, 0.32), (0.76, 0.54)] {
                p.line_segment([at(x, 0.12), at(x, 0.88)], stroke);
                p.circle_filled(at(x, knob), size * 0.09, color);
            }
        }
        "speaker" => {
            p.add(Shape::convex_polygon(
                [(0.1, 0.38), (0.26, 0.38), (0.48, 0.18), (0.48, 0.82), (0.26, 0.62), (0.1, 0.62)]
                    .iter()
                    .map(|&(x, y)| at(x, y))
                    .collect(),
                color,
                Stroke::NONE,
            ));
            for rad in [0.18, 0.32] {
                p.add(arc(0.5, 0.5, rad, -50.0, 50.0));
            }
        }
        _ => {
            p.circle_stroke(at(0.5, 0.5), size * 0.4, stroke);
        }
    }
}
