//! Draws the tiles of Home Assistant: each entity as what it is, a curtain as open as it is, the
//! dial of a thermostat, a lamp lit in its color, with its name and value beside it on the wide
//! key and under it on square ones.

use std::fmt::Write;

use super::design::{Color, CoverShape, Drawing, ImageData, LockState};
use super::hass_widgets::Figure;
use super::svg::SAFE_MARGIN;
use super::widgets::{Anchor, CAP, Chart, Font, PAD, Pen, ellipsize};

const SKY: Color = Color::rgb(96, 165, 250);
const WATER: Color = Color::rgb(43, 154, 249);
const SHADE: Color = Color::BLACK;

/// What a tile shows.
pub(super) struct Tile<'a> {
    pub(super) label: Option<&'a str>,
    pub(super) figure: Figure<'a>,
    pub(super) detail: Option<&'a str>,
    pub(super) extra: Option<&'a str>,
    pub(super) badge: Option<&'a str>,
    pub(super) tint: Option<Color>,
    pub(super) drawing: &'a Drawing,
}

/// A square to draw in, `s` on each side, measured in fractions of it.
#[derive(Clone, Copy)]
struct Frame {
    x: f32,
    y: f32,
    s: f32,
}

impl Frame {
    fn x(self, f: f32) -> f32 {
        self.x + f * self.s
    }

    fn y(self, f: f32) -> f32 {
        self.y + f * self.s
    }

    fn len(self, f: f32) -> f32 {
        f * self.s
    }

    /// A unique enough id for gradients and clips: one key holds one tile.
    fn id(self, name: &str) -> String {
        format!("{name}{}x{}", self.x.round() as i32, self.y.round() as i32)
    }
}

/// A point of an arc around `cx`, `cy`, at `angle` degrees clockwise from three o'clock.
fn polar(cx: f32, cy: f32, r: f32, angle: f32) -> (f32, f32) {
    let (sin, cos) = angle.to_radians().sin_cos();
    (cx + r * cos, cy + r * sin)
}

fn arc_path(cx: f32, cy: f32, r: f32, from: f32, sweep: f32) -> String {
    let (x0, y0) = polar(cx, cy, r, from);
    let (x1, y1) = polar(cx, cy, r, from + sweep);
    let large = u8::from(sweep > 180.0);
    format!("M{x0} {y0}A{r} {r} 0 {large} 1 {x1} {y1}")
}

impl Pen<'_> {
    pub(super) fn tile(&mut self, tile: &Tile) {
        // The drawing carries the color of the state; the value only dims while off.
        let tile = &Tile {
            figure: Figure {
                on: tile.figure.on.filter(|on| !on),
                ..tile.figure
            },
            ..*tile
        };
        match tile.drawing {
            Drawing::Gauge { level, marks, ring } => self.tile_gauge(tile, *level, marks, *ring),
            Drawing::Value => self.tile_value(tile),
            _ if self.wide() => self.tile_wide(tile),
            _ => self.tile_square(tile),
        }
    }

    fn lit(tile: &Tile) -> bool {
        tile.figure.on.is_none()
    }

    /// The drawing in a square of the key and, under it, the value with the badge beside it,
    /// and the name.
    fn tile_square(&mut self, tile: &Tile) {
        let u = self.unit;
        let pad = SAFE_MARGIN + 2.0 * u;
        let (iw, ih) = (self.w - 2.0 * pad, self.h - 2.0 * pad);
        let gap = 8.0 * u;
        let tint = tile.tint.unwrap_or(self.accent);
        let name = tile.label.map(|text| {
            Self::fitted(
                text,
                Font::new(21.0 * u, 600, self.color).dim(0.7),
                iw,
                18.0 * u,
            )
        });
        let value_size = |room: f32| {
            (32.0 * u).min(room / Self::value_width(tile.figure.value, tile.figure.unit).max(0.01))
        };
        // The badge goes beside the value while both read, and takes its place otherwise.
        let mut badge = tile.badge.map(|text| {
            let font = Font::new(24.0 * u, 700, tint).fit(text, iw * 0.5);
            (text, font)
        });
        let beside = badge.map_or(0.0, |(text, font)| {
            super::widgets::text_width(text, font) + gap
        });
        let mut figure = Some(tile.figure);
        let mut size = value_size(iw - beside);
        if let Some((text, _)) = badge.filter(|_| size < 24.0 * u) {
            figure = None;
            let font = Font::new(32.0 * u, 700, tint).fit(text, iw);
            size = font.size;
            badge = Some((text, font));
        }
        let size = size.max(20.0 * u);
        let room = iw - if figure.is_some() { beside } else { 0.0 };
        let value = ellipsize(
            tile.figure.value,
            Font::new(size, 700, self.color),
            room - tile.figure.unit.map_or(0.0, |_| size),
        );
        let footer =
            size * CAP + name.as_ref().map_or(0.0, |(_, font)| gap + font.size * CAP) + gap * 1.5;
        let s = iw.min(ih - footer);
        let frame = Frame {
            x: self.w / 2.0 - s / 2.0,
            y: pad,
            s,
        };
        self.drawing(tile, frame, tint);
        let mut y = pad + ih;
        if let Some((text, font)) = &name {
            self.text(pad, y, Anchor::Start, *font, text);
            y -= font.size * CAP + gap;
        }
        match figure {
            Some(figure) => {
                self.value(
                    pad,
                    y,
                    Anchor::Start,
                    size,
                    Figure {
                        value: &value,
                        ..figure
                    },
                );
                if let Some((text, font)) = badge {
                    self.text(pad + iw, y, Anchor::End, font, text);
                }
            }
            None => {
                if let Some((text, font)) = badge {
                    self.text(pad, y, Anchor::Start, font, text);
                }
            }
        }
    }

    /// The drawing on the left and the name, value and the lines that fit on the right.
    fn tile_wide(&mut self, tile: &Tile) {
        let u = self.unit;
        let (iw, ih) = self.inner();
        let tint = tile.tint.unwrap_or(self.accent);
        let frame = Frame {
            x: PAD,
            y: PAD,
            s: ih,
        };
        self.drawing(tile, frame, tint);
        let left = PAD + ih + 20.0 * u;
        self.lines_beside(tile, left, iw - ih - 20.0 * u, true);
    }

    /// The name, the value when `value`, and then the detail, the extra line and the badge,
    /// while the value keeps a size that reads.
    fn lines_beside(&mut self, tile: &Tile, left: f32, room: f32, value: bool) {
        let u = self.unit;
        let ih = self.h - 2.0 * PAD;
        let gap = 9.0 * u;
        let tint = tile.tint.unwrap_or(self.accent);
        let name = tile.label.map(|text| {
            Self::fitted(
                text,
                Font::new(24.0 * u, 600, self.color).dim(0.75),
                room,
                20.0 * u,
            )
        });
        let line = |text: &str, font: Font| {
            let font = font.fit(text, room);
            (font.size >= 19.0 * u).then(|| (text.to_string(), font))
        };
        // Each line with how much it matters, to give up the least first.
        let mut lines: Vec<(u8, String, Font)> = [
            tile.detail
                .and_then(|t| line(t, Font::new(23.0 * u, 500, self.color).dim(0.7)))
                .map(|(t, f)| (1, t, f)),
            tile.extra
                .and_then(|t| line(t, Font::new(21.0 * u, 500, self.color).dim(0.6)))
                .map(|(t, f)| (0, t, f)),
            tile.badge
                .and_then(|t| line(t, Font::new(25.0 * u, 700, tint)))
                .map(|(t, f)| (2, t, f)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let above = name.as_ref().map_or(0.0, |(_, f)| f.size * CAP + gap);
        let below = |lines: &[(u8, String, Font)]| -> f32 {
            lines.iter().map(|(_, _, f)| gap + f.size * CAP).sum()
        };
        let size_for = |lines: &[(u8, String, Font)]| {
            if !value {
                return 0.0;
            }
            (58.0 * u)
                .min(room / Self::value_width(tile.figure.value, tile.figure.unit).max(0.01))
                .min((ih - above - below(lines)) / CAP)
        };
        let mut size = size_for(&lines);
        while value && size < 34.0 * u && !lines.is_empty() {
            if let Some(least) = (0..lines.len()).min_by_key(|&i| lines[i].0) {
                lines.remove(least);
            }
            size = size_for(&lines);
        }
        let size = size.max(if value { 20.0 * u } else { 0.0 });
        let block = above + if value { size * CAP } else { -gap } + below(&lines);
        let mut y = PAD + (ih - block).max(0.0) / 2.0;
        if let Some((text, font)) = &name {
            y += font.size * CAP;
            self.text(left, y, Anchor::Start, *font, text);
            y += gap;
        }
        if value {
            y += size * CAP;
            let shown = ellipsize(
                tile.figure.value,
                Font::new(size, 700, self.color),
                room - tile.figure.unit.map_or(0.0, |_| size),
            );
            self.value(
                left,
                y,
                Anchor::Start,
                size,
                Figure {
                    value: &shown,
                    ..tile.figure
                },
            );
        } else {
            y -= gap;
        }
        for (_, text, font) in &lines {
            y += gap + font.size * CAP;
            self.text(left, y, Anchor::Start, *font, text);
        }
    }

    /// Only the value, large, with the name under it and the badge over it.
    fn tile_value(&mut self, tile: &Tile) {
        let u = self.unit;
        let (iw, ih) = self.inner();
        let gap = 10.0 * u;
        let tint = tile.tint.unwrap_or(self.accent);
        let name = tile.label.map(|text| {
            Self::fitted(
                text,
                Font::new(22.0 * u, 600, self.color).dim(0.75),
                iw,
                18.0 * u,
            )
        });
        let badge = tile
            .badge
            .map(|text| (text, Font::new(24.0 * u, 700, tint).fit(text, iw)));
        let around = name.as_ref().map_or(0.0, |(_, f)| f.size * CAP + gap)
            + badge.map_or(0.0, |(_, f)| f.size * CAP + gap);
        let largest = if self.wide() { 96.0 } else { 64.0 } * u;
        let (size, lines) = self.figure_size(tile.figure, iw, ih - around, largest);
        let block = around + Self::lines_height(size, &lines);
        let mut y = PAD + (ih - block).max(0.0) / 2.0;
        let center = self.w / 2.0;
        if let Some((text, font)) = badge {
            y += font.size * CAP;
            self.text(center, y, Anchor::Middle, font, text);
            y += gap;
        }
        y = self.figure_lines(center, y, Anchor::Middle, size, &lines, tile.figure);
        if let Some((text, font)) = &name {
            y += gap + font.size * CAP;
            self.text(center, y, Anchor::Middle, *font, text);
        }
    }

    /// An arc from the least to the most with the value inside, as the dial of a thermostat.
    /// On a square key it takes all of it, with the name in its opening; on the wide one, the
    /// lines go beside it.
    fn tile_gauge(&mut self, tile: &Tile, level: Option<f32>, marks: &[f32], ring: bool) {
        let u = self.unit;
        let (iw, ih) = self.inner();
        let wide = self.wide();
        let tint = tile.tint.unwrap_or(self.accent);
        let lit = Self::lit(tile);
        let stroke = 13.0 * u;
        let pad = SAFE_MARGIN + 2.0 * u;
        let side = if wide { ih } else { self.h - 2.0 * pad };
        let r = side / 2.0 - stroke / 2.0;
        let cx = if wide { PAD + side / 2.0 } else { self.w / 2.0 };
        let cy = if wide || ring {
            self.h / 2.0
        } else {
            self.h / 2.0 + 6.0 * u
        };
        let (from, sweep) = if ring { (-90.0, 360.0) } else { (135.0, 270.0) };
        let track = if ring {
            format!(
                r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{}" stroke-opacity="0.16" stroke-width="{stroke}"/>"#,
                self.color.hex()
            )
        } else {
            format!(
                r#"<path d="{}" fill="none" stroke="{}" stroke-opacity="0.16" stroke-width="{stroke}" stroke-linecap="round"/>"#,
                arc_path(cx, cy, r, from, sweep),
                self.color.hex()
            )
        };
        self.svg.push_str(&track);
        let fill = if lit { tint } else { self.color };
        if let Some(level) = level.map(|l| l.clamp(0.0, 1.0)) {
            let opacity = if lit { 1.0 } else { 0.45 };
            if level >= 0.999 && ring {
                let _ = write!(
                    self.svg,
                    r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{}" stroke-opacity="{opacity}" stroke-width="{stroke}"/>"#,
                    fill.hex()
                );
            } else if level > 0.0 {
                let _ = write!(
                    self.svg,
                    r#"<path d="{}" fill="none" stroke="{}" stroke-opacity="{opacity}" stroke-width="{stroke}" stroke-linecap="round"/>"#,
                    arc_path(cx, cy, r, from, (sweep * level).max(0.5)),
                    fill.hex()
                );
            }
        }
        for mark in marks {
            let (x, y) = polar(cx, cy, r, from + sweep * mark.clamp(0.0, 1.0));
            let _ = write!(
                self.svg,
                r#"<circle cx="{x}" cy="{y}" r="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
                stroke * 0.62,
                self.color.hex(),
                fill.hex(),
                stroke * 0.3,
            );
        }
        let inside = if ring { r * 1.7 } else { r * 1.45 } - stroke;
        let gap = 7.0 * u;
        let value_size = (if wide { 44.0 } else { 46.0 } * u)
            .min(0.98 * inside / Self::value_width(tile.figure.value, tile.figure.unit).max(0.01));
        let value = ellipsize(
            tile.figure.value,
            Font::new(value_size, 700, self.color),
            inside,
        );
        let below: Vec<(&str, Font)> = if wide {
            Vec::new()
        } else {
            [
                tile.detail
                    .map(|t| (t, Font::new(21.0 * u, 600, fill).fit(t, inside))),
                tile.badge
                    .map(|t| (t, Font::new(21.0 * u, 700, tint).fit(t, inside))),
                tile.label.filter(|_| ring).map(|t| {
                    let font = Font::new(18.0 * u, 600, self.color).dim(0.75);
                    (t, font.fit(t, inside))
                }),
            ]
            .into_iter()
            .flatten()
            .filter(|(_, font)| font.size >= 15.0 * u)
            .collect()
        };
        let block = value_size * CAP + below.iter().map(|(_, f)| gap + f.size * CAP).sum::<f32>();
        let mut y = cy - block / 2.0 + value_size * CAP;
        self.value(
            cx,
            y,
            Anchor::Middle,
            value_size,
            Figure {
                value: &value,
                ..tile.figure
            },
        );
        for (text, font) in below {
            y += gap + font.size * CAP;
            let text = ellipsize(text, font, inside);
            self.text(cx, y, Anchor::Middle, font, &text);
        }
        if wide {
            let left = PAD + side + 22.0 * u;
            let lines = Tile {
                figure: Figure {
                    value: "",
                    unit: None,
                    on: tile.figure.on,
                },
                ..*tile
            };
            self.lines_beside(&lines, left, iw - side - 22.0 * u, false);
        } else if let Some(text) = tile.label.filter(|_| !ring) {
            let (_, opening) = polar(cx, cy, r, 45.0);
            let (text, font) = Self::fitted(
                text,
                Font::new(18.0 * u, 600, self.color).dim(0.75),
                2.0 * r * 0.62,
                15.0 * u,
            );
            let y = (opening + stroke / 2.0 + font.size * CAP).min(self.h - SAFE_MARGIN);
            self.text(cx, y, Anchor::Middle, font, &text);
        }
    }

    fn drawing(&mut self, tile: &Tile, frame: Frame, tint: Color) {
        let lit = Self::lit(tile);
        match tile.drawing {
            Drawing::Icon { icon } => self.icon_drawing(icon.as_ref(), frame, tint, lit),
            Drawing::Bulb { level } => self.bulb(frame, tint, lit, *level),
            Drawing::Plug => self.plug(frame, tint, lit),
            Drawing::Switch => self.switch(frame, tint, lit),
            Drawing::Fan { level } => self.fan(frame, tint, lit, *level),
            Drawing::Cover { shape, open, tilt } => {
                self.cover(frame, *shape, open.clamp(0.0, 1.0), *tilt, tint, lit)
            }
            Drawing::Valve { open } => self.valve(frame, open.clamp(0.0, 1.0), tint),
            Drawing::Lock { state } => self.lock(frame, *state, tint),
            Drawing::Thermometer { level } => self.thermometer(frame, level.clamp(0.0, 1.0), tint),
            Drawing::Droplet { level } => self.droplet(frame, level.clamp(0.0, 1.0), tint),
            Drawing::Battery { level } => self.battery_cell(frame, level.clamp(0.0, 1.0), tint),
            Drawing::Picture { image, round, icon } => match image {
                Some(image) => self.photo(frame, image, *round, tint, lit),
                None => self.icon_drawing(icon.as_ref(), frame, tint, lit),
            },
            Drawing::Sun { x, height } => self.sun_sky(frame, *x, *height, tint),
            Drawing::Vacuum => self.vacuum(frame, tint, lit),
            Drawing::Gauge { .. } | Drawing::Value => {}
        }
    }

    fn rect(&mut self, f: Frame, (x, y, w, h): (f32, f32, f32, f32), rx: f32, paint: &str) {
        let _ = write!(
            self.svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" {paint}/>"#,
            f.x(x),
            f.y(y),
            f.len(w),
            f.len(h),
            f.len(rx),
        );
    }

    fn circle(&mut self, f: Frame, (x, y): (f32, f32), r: f32, paint: &str) {
        let _ = write!(
            self.svg,
            r#"<circle cx="{}" cy="{}" r="{}" {paint}/>"#,
            f.x(x),
            f.y(y),
            f.len(r),
        );
    }

    fn line(&mut self, f: Frame, (x1, y1): (f32, f32), (x2, y2): (f32, f32), paint: &str) {
        let _ = write!(
            self.svg,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {paint}/>"#,
            f.x(x1),
            f.y(y1),
            f.x(x2),
            f.y(y2),
        );
    }

    fn polygon(&mut self, f: Frame, points: &[(f32, f32)], paint: &str) {
        let points: Vec<String> = points
            .iter()
            .map(|(x, y)| format!("{},{}", f.x(*x), f.y(*y)))
            .collect();
        let _ = write!(
            self.svg,
            r#"<polygon points="{}" {paint}/>"#,
            points.join(" ")
        );
    }

    fn fill(color: Color, opacity: f32) -> String {
        format!(r#"fill="{}" fill-opacity="{opacity}""#, color.hex())
    }

    fn stroke(f: Frame, color: Color, opacity: f32, width: f32) -> String {
        format!(
            r#"fill="none" stroke="{}" stroke-opacity="{opacity}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""#,
            color.hex(),
            f.len(width)
        )
    }

    /// A soft light behind what is on.
    fn glow(&mut self, f: Frame, (x, y): (f32, f32), r: f32, color: Color, strength: f32) {
        let id = f.id("glow");
        let _ = write!(
            self.svg,
            r#"<radialGradient id="{id}"><stop offset="0" stop-color="{hex}" stop-opacity="{strength}"/><stop offset="1" stop-color="{hex}" stop-opacity="0"/></radialGradient><circle cx="{}" cy="{}" r="{}" fill="url(#{id})"/>"#,
            f.x(x),
            f.y(y),
            f.len(r),
            hex = color.hex(),
        );
    }

    fn icon_drawing(&mut self, icon: Option<&ImageData>, f: Frame, tint: Color, lit: bool) {
        let size = f.len(0.84);
        let (x, y) = (f.x(0.08), f.y(0.08));
        if icon.is_some() {
            self.icon_circle(icon, x, y, size, tint, !lit);
        } else {
            let color = if lit { tint } else { self.color };
            self.circle(
                f,
                (0.5, 0.5),
                0.42,
                &Self::fill(color, if lit { 0.22 } else { 0.1 }),
            );
            self.circle(f, (0.5, 0.5), 0.12, &Self::fill(color, 0.9));
        }
    }

    fn bulb(&mut self, f: Frame, tint: Color, lit: bool, level: Option<f32>) {
        let strength = 0.35 + 0.65 * level.unwrap_or(1.0).clamp(0.0, 1.0);
        let (cx, cy, r) = (0.5, 0.4, 0.25);
        if lit {
            self.glow(f, (cx, cy), 0.5, tint, 0.75 * strength);
            for angle in [180.0, 215.0, 250.0, 290.0, 325.0, 0.0_f32] {
                let (sin, cos) = angle.to_radians().sin_cos();
                let (inner, outer) = (r + 0.07, r + 0.07 + 0.08 * strength);
                self.line(
                    f,
                    (cx + inner * cos, cy + inner * sin),
                    (cx + outer * cos, cy + outer * sin),
                    &Self::stroke(f, tint, 0.9, 0.035),
                );
            }
        }
        let (lx, ly) = polar(cx, cy, r, 125.0);
        let (rx, ry) = polar(cx, cy, r, 55.0);
        let glass = format!(
            "M{} {}A{} {} 0 1 1 {} {}L{} {}L{} {}Z",
            f.x(lx),
            f.y(ly),
            f.len(r),
            f.len(r),
            f.x(rx),
            f.y(ry),
            f.x(0.585),
            f.y(0.7),
            f.x(0.415),
            f.y(0.7),
        );
        let paint = if lit {
            Self::fill(tint, 1.0)
        } else {
            Self::stroke(f, self.color, 0.6, 0.035)
        };
        let _ = write!(self.svg, r#"<path d="{glass}" {paint}/>"#);
        let base = Self::fill(self.color, if lit { 0.85 } else { 0.55 });
        self.rect(f, (0.4, 0.725, 0.2, 0.045), 0.015, &base);
        self.rect(f, (0.4, 0.785, 0.2, 0.045), 0.015, &base);
        self.rect(f, (0.45, 0.845, 0.1, 0.035), 0.015, &base);
    }

    fn plug(&mut self, f: Frame, tint: Color, lit: bool) {
        self.rect(
            f,
            (0.12, 0.12, 0.76, 0.76),
            0.17,
            &format!(
                r#"{} stroke="{}" stroke-opacity="0.35" stroke-width="{}""#,
                Self::fill(self.color, 0.08),
                self.color.hex(),
                f.len(0.02)
            ),
        );
        if lit {
            self.glow(f, (0.5, 0.5), 0.46, tint, 0.55);
        }
        let socket = if lit {
            Self::fill(tint, 1.0)
        } else {
            Self::fill(self.color, 0.2)
        };
        self.circle(f, (0.5, 0.5), 0.27, &socket);
        let clip = Self::fill(self.color, if lit { 0.9 } else { 0.45 });
        self.rect(f, (0.46, 0.225, 0.08, 0.045), 0.01, &clip);
        self.rect(f, (0.46, 0.73, 0.08, 0.045), 0.01, &clip);
        let hole = Self::fill(SHADE, 0.65);
        self.circle(f, (0.39, 0.5), 0.045, &hole);
        self.circle(f, (0.61, 0.5), 0.045, &hole);
    }

    fn switch(&mut self, f: Frame, tint: Color, lit: bool) {
        let track = if lit {
            Self::fill(tint, 1.0)
        } else {
            Self::fill(self.color, 0.18)
        };
        self.rect(f, (0.12, 0.3, 0.76, 0.4), 0.2, &track);
        let (knob, paint) = if lit {
            ((0.68, 0.5), Self::fill(Color::WHITE, 1.0))
        } else {
            ((0.32, 0.5), Self::fill(self.color, 0.7))
        };
        self.circle(f, knob, 0.15, &paint);
    }

    fn fan(&mut self, f: Frame, tint: Color, lit: bool, level: Option<f32>) {
        let color = if lit { tint } else { self.color };
        let opacity = if lit { 1.0 } else { 0.5 };
        let (cx, cy) = (f.x(0.5), f.y(0.5));
        let blade = format!(
            "M{cx} {cy}C{} {} {} {} {} {}C{} {} {} {} {cx} {cy}Z",
            f.x(0.38),
            f.y(0.36),
            f.x(0.37),
            f.y(0.16),
            f.x(0.5),
            f.y(0.13),
            f.x(0.64),
            f.y(0.15),
            f.x(0.63),
            f.y(0.34),
        );
        for angle in [0.0, 120.0, 240.0] {
            let _ = write!(
                self.svg,
                r#"<path d="{blade}" {} transform="rotate({} {cx} {cy})"/>"#,
                Self::fill(color, opacity),
                angle + 15.0,
            );
        }
        self.circle(f, (0.5, 0.5), 0.075, &Self::fill(self.color, 0.9));
        if let Some(level) = level.filter(|_| lit) {
            let r = f.len(0.46);
            let _ = write!(
                self.svg,
                r#"<circle cx="{cx}" cy="{cy}" r="{r}" {}/>"#,
                Self::stroke(f, self.color, 0.16, 0.035)
            );
            if level > 0.0 {
                let _ = write!(
                    self.svg,
                    r#"<path d="{}" {}/>"#,
                    arc_path(cx, cy, r, -90.0, (360.0 * level.min(0.999)).max(1.0)),
                    Self::stroke(f, tint, 1.0, 0.035)
                );
            }
        }
    }

    /// Glass of a window with what is behind it, light while there is light to see.
    fn glass(&mut self, f: Frame, (x, y, w, h): (f32, f32, f32, f32)) {
        self.rect(f, (x, y, w, h), 0.0, &Self::fill(SKY, 0.4));
        let bars = Self::stroke(f, self.color, 0.3, 0.02);
        self.line(f, (x + w / 2.0, y), (x + w / 2.0, y + h), &bars);
        self.line(f, (x, y + h / 2.0), (x + w, y + h / 2.0), &bars);
    }

    fn cover(
        &mut self,
        f: Frame,
        shape: CoverShape,
        open: f32,
        tilt: Option<f32>,
        tint: Color,
        lit: bool,
    ) {
        let fabric = Self::fill(self.color, 0.88);
        let seam = Self::stroke(f, SHADE, 0.28, 0.012);
        let frame = Self::stroke(f, self.color, 0.6, 0.03);
        let closed = 1.0 - open;
        match shape {
            CoverShape::Curtain => {
                self.glass(f, (0.17, 0.15, 0.66, 0.72));
                let width = 0.06 + 0.3 * closed;
                for x in [0.13, 0.87 - width] {
                    self.rect(f, (x, 0.12, width, 0.78), 0.015, &fabric);
                    for fold in 1..4 {
                        let fx = x + width * fold as f32 / 4.0;
                        self.line(f, (fx, 0.14), (fx, 0.88), &seam);
                    }
                }
                self.line(
                    f,
                    (0.08, 0.12),
                    (0.92, 0.12),
                    &Self::stroke(f, self.color, 1.0, 0.04),
                );
            }
            CoverShape::Blind => {
                self.glass(f, (0.15, 0.14, 0.7, 0.74));
                self.rect(f, (0.12, 0.1, 0.76, 0.05), 0.012, &fabric);
                let bottom = 0.15 + 0.73 * closed;
                let thick = 0.012 + 0.04 * (1.0 - tilt.unwrap_or(0.4).clamp(0.0, 1.0));
                let mut y = 0.17;
                while y + thick < bottom - 0.02 {
                    self.rect(f, (0.14, y, 0.72, thick), 0.006, &fabric);
                    y += 0.055;
                }
                self.rect(f, (0.13, bottom - 0.025, 0.74, 0.03), 0.01, &fabric);
            }
            CoverShape::Shutter => {
                self.glass(f, (0.15, 0.2, 0.7, 0.68));
                let side = Self::stroke(f, self.color, 0.55, 0.025);
                self.line(f, (0.135, 0.2), (0.135, 0.88), &side);
                self.line(f, (0.865, 0.2), (0.865, 0.88), &side);
                let bottom = 0.2 + 0.68 * closed;
                if closed > 0.0 {
                    self.rect(f, (0.15, 0.2, 0.7, bottom - 0.2), 0.0, &fabric);
                    let mut y = 0.2 + 0.05;
                    while y < bottom - 0.01 {
                        self.line(f, (0.15, y), (0.85, y), &seam);
                        y += 0.05;
                    }
                }
                self.rect(
                    f,
                    (0.14, bottom - 0.02, 0.72, 0.03),
                    0.008,
                    &Self::fill(self.color, 1.0),
                );
                self.rect(
                    f,
                    (0.1, 0.08, 0.8, 0.12),
                    0.025,
                    &Self::fill(self.color, 0.95),
                );
            }
            CoverShape::Shade => {
                self.glass(f, (0.15, 0.14, 0.7, 0.74));
                let bottom = 0.16 + 0.72 * closed;
                self.rect(
                    f,
                    (0.16, 0.14, 0.68, bottom - 0.14),
                    0.0,
                    &Self::fill(self.color, 0.8),
                );
                self.rect(
                    f,
                    (0.15, bottom - 0.02, 0.7, 0.035),
                    0.012,
                    &Self::fill(self.color, 1.0),
                );
                self.line(
                    f,
                    (0.5, bottom + 0.015),
                    (0.5, bottom + 0.06),
                    &Self::stroke(f, self.color, 0.8, 0.015),
                );
                self.circle(f, (0.5, bottom + 0.07), 0.018, &Self::fill(self.color, 1.0));
                self.rect(
                    f,
                    (0.11, 0.09, 0.78, 0.06),
                    0.03,
                    &Self::fill(self.color, 1.0),
                );
            }
            CoverShape::Garage => {
                let roof = Self::stroke(f, self.color, 0.85, 0.045);
                self.polygon(f, &[(0.08, 0.4), (0.5, 0.1), (0.92, 0.4)], &roof);
                let wall = Self::stroke(f, self.color, 0.6, 0.035);
                self.line(f, (0.17, 0.34), (0.17, 0.9), &wall);
                self.line(f, (0.83, 0.34), (0.83, 0.9), &wall);
                self.rect(f, (0.25, 0.45, 0.5, 0.45), 0.0, &Self::fill(SHADE, 0.45));
                if lit {
                    self.rect(f, (0.25, 0.45, 0.5, 0.45), 0.0, &Self::fill(tint, 0.25));
                }
                let bottom = 0.45 + 0.45 * closed;
                if closed > 0.0 {
                    self.rect(f, (0.25, 0.45, 0.5, bottom - 0.45), 0.0, &fabric);
                    let mut y = 0.45 + 0.075;
                    while y < bottom - 0.01 {
                        self.line(f, (0.25, y), (0.75, y), &seam);
                        y += 0.075;
                    }
                }
            }
            CoverShape::Awning => {
                self.rect(f, (0.3, 0.58, 0.4, 0.3), 0.0, &Self::fill(SKY, 0.35));
                self.rect(f, (0.3, 0.58, 0.4, 0.3), 0.0, &frame);
                self.rect(f, (0.1, 0.1, 0.8, 0.05), 0.02, &Self::fill(self.color, 1.0));
                let depth = 0.06 + 0.32 * open;
                let stripe = 0.8 / 6.0;
                for i in 0..6 {
                    let paint = if i % 2 == 0 {
                        Self::fill(tint, 1.0)
                    } else {
                        Self::fill(self.color, 0.9)
                    };
                    let x = 0.1 + stripe * i as f32;
                    self.rect(f, (x, 0.15, stripe, depth), 0.0, &paint);
                    self.circle(f, (x + stripe / 2.0, 0.15 + depth), stripe / 2.0, &paint);
                }
            }
            CoverShape::Gate => {
                let ground = Self::stroke(f, self.color, 0.4, 0.02);
                self.line(f, (0.05, 0.88), (0.95, 0.88), &ground);
                let post = Self::fill(self.color, 0.9);
                self.rect(f, (0.08, 0.28, 0.06, 0.6), 0.01, &post);
                self.rect(f, (0.86, 0.28, 0.06, 0.6), 0.01, &post);
                let end = 0.14 + 0.72 * closed;
                if closed > 0.02 {
                    let bar = Self::stroke(f, self.color, 0.85, 0.025);
                    self.line(f, (0.14, 0.38), (end, 0.38), &bar);
                    self.line(f, (0.14, 0.8), (end, 0.8), &bar);
                    let mut x = 0.18;
                    while x < end {
                        self.line(f, (x, 0.34), (x, 0.84), &bar);
                        x += 0.08;
                    }
                }
            }
            CoverShape::Door => {
                self.rect(f, (0.26, 0.1, 0.48, 0.8), 0.0, &Self::fill(SHADE, 0.4));
                if open > 0.0 {
                    self.rect(
                        f,
                        (0.26, 0.1, 0.48, 0.8),
                        0.0,
                        &Self::fill(tint, if lit { 0.4 } else { 0.15 }),
                    );
                    let edge = 0.26 + 0.48 * (1.0 - 0.7 * open);
                    let leaf = [(0.26, 0.1), (edge, 0.15), (edge, 0.85), (0.26, 0.9)];
                    self.polygon(f, &leaf, &Self::fill(self.color, 0.85));
                    self.circle(f, (edge - 0.04, 0.52), 0.022, &Self::fill(SHADE, 0.6));
                } else {
                    self.rect(
                        f,
                        (0.26, 0.1, 0.48, 0.8),
                        0.0,
                        &Self::fill(self.color, 0.85),
                    );
                    self.circle(f, (0.67, 0.52), 0.025, &Self::fill(SHADE, 0.6));
                }
                self.rect(f, (0.26, 0.1, 0.48, 0.8), 0.0, &frame);
            }
            CoverShape::Window => {
                self.glass(f, (0.16, 0.14, 0.68, 0.72));
                if open > 0.0 {
                    self.rect(
                        f,
                        (0.5, 0.14, 0.34, 0.72),
                        0.0,
                        &Self::fill(tint, if lit { 0.35 } else { 0.15 }),
                    );
                    let edge = 0.5 + 0.34 * (1.0 - 0.6 * open);
                    let pane = [(0.84, 0.14), (edge, 0.08), (edge, 0.92), (0.84, 0.86)];
                    self.polygon(f, &pane, &Self::fill(SKY, 0.5));
                    self.polygon(f, &pane, &frame);
                }
                self.rect(f, (0.16, 0.14, 0.68, 0.72), 0.0, &frame);
                self.line(f, (0.5, 0.14), (0.5, 0.86), &frame);
            }
        }
    }

    fn valve(&mut self, f: Frame, open: f32, tint: Color) {
        self.rect(
            f,
            (0.06, 0.42, 0.88, 0.16),
            0.02,
            &Self::fill(self.color, 0.25),
        );
        if open > 0.0 {
            self.rect(
                f,
                (0.06, 0.46, 0.88, 0.08),
                0.0,
                &Self::fill(WATER, 0.4 + 0.6 * open),
            );
        }
        self.circle(f, (0.5, 0.5), 0.15, &Self::fill(self.color, 0.9));
        self.line(
            f,
            (0.5, 0.36),
            (0.5, 0.24),
            &Self::stroke(f, self.color, 0.9, 0.04),
        );
        let lever = if open > 0.0 { tint } else { self.color };
        if open > 0.5 {
            self.rect(f, (0.26, 0.17, 0.48, 0.07), 0.035, &Self::fill(lever, 1.0));
        } else {
            self.rect(f, (0.465, 0.02, 0.07, 0.26), 0.035, &Self::fill(lever, 1.0));
        }
    }

    fn lock(&mut self, f: Frame, state: LockState, tint: Color) {
        let shackle = match state {
            LockState::Locked => "M0.34 0.46V0.32A0.16 0.16 0 0 1 0.66 0.32V0.46",
            LockState::Moving => "M0.66 0.46V0.28A0.16 0.16 0 0 0 0.34 0.28V0.38",
            LockState::Unlocked | LockState::Jammed => {
                "M0.66 0.46V0.24A0.16 0.16 0 0 0 0.34 0.24V0.3"
            }
        };
        let _ = write!(
            self.svg,
            r#"<g transform="translate({} {}) scale({})"><path d="{shackle}" fill="none" stroke="{}" stroke-width="0.065" stroke-linecap="round"/></g>"#,
            f.x,
            f.y,
            f.s,
            self.color.hex(),
        );
        self.rect(f, (0.22, 0.44, 0.56, 0.44), 0.08, &Self::fill(tint, 1.0));
        let hole = Self::fill(SHADE, 0.55);
        self.circle(f, (0.5, 0.62), 0.05, &hole);
        self.rect(f, (0.48, 0.62, 0.04, 0.12), 0.015, &hole);
        if state == LockState::Jammed {
            self.line(
                f,
                (0.3, 0.52),
                (0.7, 0.8),
                &Self::stroke(f, Color::WHITE, 0.9, 0.04),
            );
        }
    }

    fn thermometer(&mut self, f: Frame, level: f32, tint: Color) {
        self.rect(
            f,
            (0.41, 0.08, 0.18, 0.66),
            0.09,
            &format!(
                r#"{} stroke="{}" stroke-opacity="0.6" stroke-width="{}""#,
                Self::fill(self.color, 0.08),
                self.color.hex(),
                f.len(0.025)
            ),
        );
        let top = 0.68 - 0.56 * level;
        self.rect(
            f,
            (0.465, top, 0.07, 0.74 - top),
            0.035,
            &Self::fill(tint, 1.0),
        );
        self.circle(f, (0.5, 0.78), 0.13, &Self::fill(tint, 1.0));
        let tick = Self::stroke(f, self.color, 0.5, 0.018);
        for i in 0..5 {
            let y = 0.16 + 0.12 * i as f32;
            self.line(f, (0.64, y), (0.7, y), &tick);
        }
    }

    fn droplet(&mut self, f: Frame, level: f32, tint: Color) {
        let drop = format!(
            "M{} {}C{} {} {} {} {} {}A{} {} 0 0 0 {} {}C{} {} {} {} {} {}Z",
            f.x(0.5),
            f.y(0.08),
            f.x(0.42),
            f.y(0.2),
            f.x(0.2),
            f.y(0.44),
            f.x(0.2),
            f.y(0.6),
            f.len(0.3),
            f.len(0.3),
            f.x(0.8),
            f.y(0.6),
            f.x(0.8),
            f.y(0.44),
            f.x(0.58),
            f.y(0.2),
            f.x(0.5),
            f.y(0.08),
        );
        let id = f.id("drop");
        let top = 0.9 - 0.82 * level;
        let _ = write!(
            self.svg,
            r#"<clipPath id="{id}"><path d="{drop}"/></clipPath><path d="{drop}" {}/><rect x="{}" y="{}" width="{}" height="{}" fill="{}" clip-path="url(#{id})"/><path d="{drop}" {}/>"#,
            Self::fill(self.color, 0.08),
            f.x(0.0),
            f.y(top),
            f.len(1.0),
            f.len(1.0 - top),
            tint.hex(),
            Self::stroke(f, self.color, 0.6, 0.03),
        );
    }

    fn battery_cell(&mut self, f: Frame, level: f32, tint: Color) {
        self.rect(
            f,
            (0.42, 0.08, 0.16, 0.07),
            0.02,
            &Self::fill(self.color, 0.7),
        );
        self.rect(
            f,
            (0.29, 0.15, 0.42, 0.75),
            0.07,
            &Self::stroke(f, self.color, 0.7, 0.035),
        );
        let height = 0.64 * level;
        if height > 0.0 {
            self.rect(
                f,
                (0.345, 0.85 - height, 0.31, height),
                0.035,
                &Self::fill(tint, 1.0),
            );
        }
    }

    fn photo(&mut self, f: Frame, image: &ImageData, round: bool, tint: Color, lit: bool) {
        let id = f.id("photo");
        let (inset, size) = (0.04, 0.92);
        let shape = if round {
            format!(
                r#"<circle cx="{}" cy="{}" r="{}"/>"#,
                f.x(0.5),
                f.y(0.5),
                f.len(size / 2.0)
            )
        } else {
            format!(
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}"/>"#,
                f.x(inset),
                f.y(inset),
                f.len(size),
                f.len(size),
                f.len(0.08)
            )
        };
        let _ = write!(
            self.svg,
            r#"<clipPath id="{id}">{shape}</clipPath><g clip-path="url(#{id})">"#
        );
        let area = Chart {
            x: f.x(inset),
            y: f.y(inset),
            w: f.len(size),
            h: f.len(size),
        };
        self.image(image, area, "xMidYMid slice", None);
        if !lit {
            self.rect(f, (0.0, 0.0, 1.0, 1.0), 0.0, &Self::fill(SHADE, 0.45));
        }
        self.svg.push_str("</g>");
        if round {
            self.circle(
                f,
                (0.5, 0.5),
                size / 2.0,
                &Self::stroke(
                    f,
                    if lit { tint } else { self.color },
                    if lit { 1.0 } else { 0.4 },
                    0.04,
                ),
            );
        }
    }

    fn sun_sky(&mut self, f: Frame, x: f32, height: f32, tint: Color) {
        let horizon = 0.66;
        let sky = format!(
            r#"<path d="M{} {}A{} {} 0 0 1 {} {}" {} stroke-dasharray="{} {}"/>"#,
            f.x(0.1),
            f.y(horizon),
            f.len(0.4),
            f.len(0.4),
            f.x(0.9),
            f.y(horizon),
            Self::stroke(f, self.color, 0.3, 0.02),
            f.len(0.03),
            f.len(0.04),
        );
        self.svg.push_str(&sky);
        let up = height >= 0.0;
        let (cx, cy) = (
            0.1 + 0.8 * x.clamp(0.0, 1.0),
            horizon - 0.4 * height.clamp(-0.6, 1.0),
        );
        if up {
            self.glow(f, (cx, cy), 0.22, tint, 0.6);
        }
        self.circle(
            f,
            (cx, cy),
            0.08,
            &Self::fill(tint, if up { 1.0 } else { 0.35 }),
        );
        self.rect(
            f,
            (0.0, horizon, 1.0, 0.3),
            0.0,
            &Self::fill(SHADE, if up { 0.0 } else { 0.35 }),
        );
        self.line(
            f,
            (0.04, horizon),
            (0.96, horizon),
            &Self::stroke(f, self.color, 0.6, 0.025),
        );
    }

    fn vacuum(&mut self, f: Frame, tint: Color, lit: bool) {
        self.circle(
            f,
            (0.5, 0.5),
            0.38,
            &format!(
                r#"{} stroke="{}" stroke-opacity="0.6" stroke-width="{}""#,
                Self::fill(self.color, 0.1),
                self.color.hex(),
                f.len(0.03)
            ),
        );
        let (cx, cy) = (f.x(0.5), f.y(0.5));
        let bumper = if lit { tint } else { self.color };
        let _ = write!(
            self.svg,
            r#"<path d="{}" {}/>"#,
            arc_path(cx, cy, f.len(0.38), 205.0, 130.0),
            Self::stroke(f, bumper, if lit { 1.0 } else { 0.5 }, 0.06)
        );
        self.circle(f, (0.5, 0.42), 0.08, &Self::fill(self.color, 0.6));
        self.circle(
            f,
            (0.5, 0.64),
            0.035,
            &Self::fill(if lit { tint } else { self.color }, 0.9),
        );
    }
}
