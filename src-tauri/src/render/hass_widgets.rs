//! Draws the widgets of Home Assistant: a value with its icon, a history, several values, a
//! picture and the weather. Same layouts as the rest: side by side on the wide key, stacked on
//! square ones.

use std::f32::consts::PI;
use std::fmt::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::design::{Color, ImageData, Reading, WeatherDay};
use super::svg::{SAFE_MARGIN, escape, tinted};
use super::widgets::{Anchor, CAP, Chart, Font, PAD, Pen, ellipsize, text_width, wrap};

/// The unit next to a value, as a fraction of the value's size.
const UNIT: f32 = 0.42;

/// What an entity's card shows.
pub(super) struct Card<'a> {
    pub(super) label: Option<&'a str>,
    pub(super) figure: Figure<'a>,
    pub(super) detail: Option<&'a str>,
    pub(super) extra: Option<&'a str>,
    pub(super) level: Option<f32>,
    pub(super) tint: Option<Color>,
    pub(super) icon: Option<&'a ImageData>,
}

/// A value as it is drawn: its text, its unit and whether what it shows is on.
#[derive(Clone, Copy)]
pub(super) struct Figure<'a> {
    pub(super) value: &'a str,
    pub(super) unit: Option<&'a str>,
    pub(super) on: Option<bool>,
}

impl<'a> Figure<'a> {
    fn plain(value: &'a str, unit: Option<&'a str>) -> Self {
        Self {
            value,
            unit,
            on: None,
        }
    }
}

impl Pen<'_> {
    fn image(&mut self, image: &ImageData, area: Chart, aspect: &str, tint: Option<Color>) {
        let Chart { x, y, w, h } = area;
        let _ = write!(
            self.svg,
            r#"<image x="{x}" y="{y}" width="{w}" height="{h}" preserveAspectRatio="{aspect}" href="data:{};base64,{}"/>"#,
            image.format.mime(),
            STANDARD.encode(tinted(image, tint)),
        );
    }

    fn value_color(&self, on: Option<bool>) -> (Color, f32) {
        match on {
            Some(true) => (self.accent, 1.0),
            Some(false) => (self.color, 0.62),
            None => (self.color, 1.0),
        }
    }

    /// `text` in `font`, shrunk to fit `width` but not below `least`, and cut there.
    fn fitted(text: &str, font: Font, width: f32, least: f32) -> (String, Font) {
        let font = font.sized(font.fit(text, width).size.max(least));
        (ellipsize(text, font, width), font)
    }

    /// The name over a value: in the accent, as written, never smaller than reads on the dock.
    fn name(&self, text: &str, size: f32, width: f32) -> (String, Font) {
        let u = self.unit;
        Self::fitted(text, Font::new(size * u, 600, self.accent), width, 17.0 * u)
    }

    /// The width of a value and its unit at size 1.
    fn value_width(value: &str, unit: Option<&str>) -> f32 {
        let font = Font::new(1.0, 700, Color::WHITE);
        text_width(value, font)
            + unit.map_or(0.0, |unit| {
                0.12 + text_width(unit, Font::new(UNIT, 600, Color::WHITE))
            })
    }

    /// A value with its unit right after it, from `x` as `anchor` says, on the baseline `y`.
    fn value(&mut self, x: f32, y: f32, anchor: Anchor, size: f32, figure: Figure) {
        let Figure { value, unit, on } = figure;
        let (color, opacity) = self.value_color(on);
        let font = Font::new(size, 700, color).dim(opacity);
        let unit = unit.map(|unit| {
            format!(
                r#"<tspan dx="{}" font-size="{}" font-weight="600" fill="{}" fill-opacity="0.7">{}</tspan>"#,
                0.12 * size,
                size * UNIT,
                self.color.hex(),
                escape(unit),
            )
        });
        let _ = write!(
            self.svg,
            r#"<text x="{x}" y="{y}" text-anchor="{}" {}>{}{}</text>"#,
            anchor.name(),
            font.attributes(&self.family),
            escape(value),
            unit.unwrap_or_default(),
        );
    }

    /// Lines of a value too long for one line at a readable size, and the size that fits them.
    fn lines(
        &self,
        value: &str,
        width: f32,
        height: f32,
        largest: f32,
        smallest: f32,
    ) -> (f32, Vec<String>) {
        let one = largest.min(width / Self::value_width(value, None).max(0.01));
        if one >= smallest || !value.contains(' ') {
            return (one.min(height / CAP), vec![value.to_string()]);
        }
        let mut size = largest;
        loop {
            let font = Font::new(size, 700, self.color);
            let lines = wrap(value, font, width);
            let tall = size * CAP * lines.len() as f32 + size * 0.3 * (lines.len() as f32 - 1.0);
            if (lines.len() <= 3
                && tall <= height
                && lines.iter().all(|l| text_width(l, font) <= width))
                || size <= smallest * 0.6
            {
                return (
                    size,
                    lines.iter().map(|l| ellipsize(l, font, width)).collect(),
                );
            }
            size -= 2.0 * self.unit;
        }
    }

    /// An entity as a card: its icon in a circle of the color of its state, its name, its value
    /// and, while there is room for them at a size that reads on the dock, a line more and a bar
    /// for values with a range. What does not fit is left out rather than drawn tiny.
    pub(super) fn entity(&mut self, card: &Card) {
        if !self.wide() {
            self.entity_square(card);
            return;
        }
        let (iw, ih) = self.inner();
        let u = self.unit;
        let off = card.figure.on == Some(false);
        let tint = card.tint.unwrap_or(self.accent);
        let circle = card.icon.map(|_| ih * 0.6);
        let (left, room) = match circle {
            Some(size) => (PAD + size + 18.0 * u, iw - size - 18.0 * u),
            None => (PAD, iw),
        };
        let gap = 9.0 * u;
        let label = card.label.map(|text| {
            Self::fitted(
                text,
                Font::new(25.0 * u, 600, self.color).dim(0.75),
                room,
                22.0 * u,
            )
        });
        let line = |text: &str| {
            let font = Font::new(23.0 * u, 500, self.color)
                .dim(0.7)
                .fit(text, room);
            (font.size >= 22.0 * u).then(|| (text.to_string(), font))
        };
        let mut bar = card.level.map(|level| (level, 9.0 * u));
        // Under the value there is room for two things more: the bar first, as it reads at a
        // glance.
        let mut more = [card.detail, card.extra]
            .into_iter()
            .flatten()
            .filter_map(line);
        let mut detail = more.next();
        let mut extra = more.next().filter(|_| bar.is_none());
        let above = label.as_ref().map_or(0.0, |(_, f)| f.size * CAP + gap);
        let value = |below: f32, pen: &Self| {
            let height = (ih - above - below).max(12.0 * u);
            pen.figure_size(card.figure, room, height, 70.0 * u)
        };
        let below = |detail: &Option<(String, Font)>,
                     extra: &Option<(String, Font)>,
                     bar: Option<(f32, f32)>| {
            detail
                .as_ref()
                .map_or(0.0, |(_, f)| gap * 1.4 + f.size * CAP)
                + extra.as_ref().map_or(0.0, |(_, f)| gap + f.size * CAP)
                + bar.map_or(0.0, |(_, h)| gap * 1.6 + h)
        };
        let (mut size, mut lines) = value(below(&detail, &extra, bar), self);
        // Lines are only given up while the value is short of height, not of width.
        let widest = value(0.0, self).0;
        while size < (36.0 * u).min(widest - 0.5)
            && (extra.is_some() || detail.is_some() || bar.is_some())
        {
            if extra.is_some() {
                extra = None;
            } else if bar.is_some() && detail.is_some() {
                detail = None;
            } else {
                detail = None;
                bar = None;
            }
            (size, lines) = value(below(&detail, &extra, bar), self);
        }
        let text_height = above + Self::lines_height(size, &lines) + below(&detail, &extra, bar);
        if let Some(size) = circle {
            self.icon_circle(card.icon, PAD, PAD + (ih - size) / 2.0, size, tint, off);
        }
        let mut y = PAD + (ih - text_height).max(0.0) / 2.0;
        if let Some((text, font)) = &label {
            y += font.size * CAP;
            self.text(left, y, Anchor::Start, *font, text);
            y += gap;
        }
        y = self.figure_lines(left, y, Anchor::Start, size, &lines, card.figure);
        if let Some((text, font)) = &detail {
            y += gap * 1.4 + font.size * CAP;
            self.text(left, y, Anchor::Start, *font, text);
        }
        if let Some((text, font)) = &extra {
            y += gap + font.size * CAP;
            self.text(left, y, Anchor::Start, *font, text);
        }
        if let Some((level, height)) = bar {
            y += gap * 1.6;
            self.bar(left, y, room, height, level, tint);
        }
    }

    /// A card on a square key, in three bands: the icon and, beside it, the one thing more that
    /// fits; the value, as large as it goes; and the name along the bottom, with the bar over it.
    fn entity_square(&mut self, card: &Card) {
        let u = self.unit;
        // Square keys are tight: their card goes closer to the edges than other widgets.
        let pad = SAFE_MARGIN + 2.0 * u;
        let (iw, ih) = (self.w - 2.0 * pad, self.h - 2.0 * pad);
        let off = card.figure.on == Some(false);
        let tint = card.tint.unwrap_or(self.accent);
        let gap = 10.0 * u;
        let circle = card.icon.map(|_| 62.0 * u);
        let aside = iw - circle.map_or(0.0, |size| size + gap);
        let badge = card.detail.and_then(|text| {
            let color = if off { self.color } else { tint };
            let font = Font::new(30.0 * u, 700, color).fit(text, aside);
            (font.size >= 23.0 * u).then_some((text, font))
        });
        let header = circle
            .unwrap_or(0.0)
            .max(badge.map_or(0.0, |(_, font)| font.size * CAP));
        let name = card.label.map(|text| {
            Self::fitted(
                text,
                Font::new(25.0 * u, 600, self.color).dim(0.8),
                iw,
                22.0 * u,
            )
        });
        let bar = card.level.map(|level| (level, 9.0 * u));
        let top = pad + if header > 0.0 { header + gap } else { 0.0 };
        let footer = name.as_ref().map_or(0.0, |(_, font)| font.size * CAP + gap)
            + bar.map_or(0.0, |(_, height)| height + gap);
        let height = (pad + ih - footer - top).max(12.0 * u);
        let (size, lines) = self.figure_size(card.figure, iw, height, 80.0 * u);
        if let Some(size) = circle {
            self.icon_circle(card.icon, pad, pad + (header - size) / 2.0, size, tint, off);
        }
        if let Some((text, font)) = badge {
            let y = pad + (header + font.size * CAP) / 2.0;
            self.text(pad + iw, y, Anchor::End, font, text);
        }
        let y = top + (height - Self::lines_height(size, &lines)).max(0.0) / 2.0;
        self.figure_lines(pad, y, Anchor::Start, size, &lines, card.figure);
        let mut y = pad + ih;
        if let Some((text, font)) = &name {
            self.text(pad, y, Anchor::Start, *font, text);
            y -= font.size * CAP + gap;
        }
        if let Some((level, height)) = bar {
            self.bar(pad, y - height, iw, height, level, tint);
        }
    }

    /// The size and lines of a value in `width` and `height`: one line when it has a unit,
    /// wrapped into a few larger lines rather than shrunk when it is words.
    fn figure_size(
        &self,
        figure: Figure,
        width: f32,
        height: f32,
        largest: f32,
    ) -> (f32, Vec<String>) {
        match figure.unit {
            Some(_) => (
                largest
                    .min(width / Self::value_width(figure.value, figure.unit).max(0.01))
                    .min(height / CAP),
                vec![figure.value.to_string()],
            ),
            None => self.lines(figure.value, width, height, largest, 30.0 * self.unit),
        }
    }

    fn lines_height(size: f32, lines: &[String]) -> f32 {
        size * CAP * lines.len() as f32 + size * 0.3 * (lines.len() as f32 - 1.0)
    }

    /// Draws the lines of a value from `y`, its unit after the last, and gives where they end.
    fn figure_lines(
        &mut self,
        x: f32,
        mut y: f32,
        anchor: Anchor,
        size: f32,
        lines: &[String],
        figure: Figure,
    ) -> f32 {
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                y += size * 0.3;
            }
            y += size * CAP;
            let unit = if i + 1 == lines.len() {
                figure.unit
            } else {
                None
            };
            let figure = Figure {
                value: line,
                unit,
                on: figure.on.filter(|on| !on),
            };
            self.value(x, y, anchor, size, figure);
        }
        y
    }

    /// An icon in a soft circle of `tint`, the way Home Assistant's cards show the state.
    fn icon_circle(
        &mut self,
        icon: Option<&ImageData>,
        x: f32,
        y: f32,
        size: f32,
        tint: Color,
        off: bool,
    ) {
        let Some(image) = icon else {
            return;
        };
        let r = size / 2.0;
        let _ = write!(
            self.svg,
            r#"<circle cx="{}" cy="{}" r="{r}" fill="{}" fill-opacity="{}"/>"#,
            x + r,
            y + r,
            if off { self.color.hex() } else { tint.hex() },
            if off { 0.1 } else { 0.22 },
        );
        let inner = size * 0.64;
        let area = Chart {
            x: x + (size - inner) / 2.0,
            y: y + (size - inner) / 2.0,
            w: inner,
            h: inner,
        };
        let color = if off { self.color } else { tint };
        self.image(image, area, "xMidYMid meet", Some(color));
    }

    pub(super) fn history(
        &mut self,
        label: Option<&str>,
        value: &str,
        unit: Option<&str>,
        points: &[Option<f32>],
        low: &str,
        high: &str,
    ) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let scale = Font::new(16.0 * u, 600, self.color).dim(0.6);
        if self.wide() {
            let side = iw * 0.42;
            let size = (58.0 * u).min(side / Self::value_width(value, unit).max(0.01));
            let label = label.map(|text| self.name(text, 21.0, side));
            let gap = 14.0 * u;
            let block = size * CAP + label.as_ref().map_or(0.0, |(_, f)| gap + f.size * CAP);
            let mut y = PAD + (ih - block) / 2.0;
            if let Some((text, font)) = &label {
                y += font.size * CAP;
                self.text(PAD, y, Anchor::Start, *font, text);
                y += gap;
            }
            y += size * CAP;
            self.value(PAD, y, Anchor::Start, size, Figure::plain(value, unit));
            let top = scale.size * CAP + 6.0 * u;
            let chart = Chart {
                x: PAD + side + 16.0 * u,
                y: PAD + top,
                w: iw - side - 16.0 * u,
                h: ih - 2.0 * top,
            };
            self.chart_frame(chart);
            let accent = self.accent;
            self.series(chart, points, accent, true);
            self.text(
                chart.x + chart.w,
                PAD + scale.size * CAP,
                Anchor::End,
                scale,
                high,
            );
            self.text(chart.x + chart.w, PAD + ih, Anchor::End, scale, low);
            return;
        }
        let size = (44.0 * u).min(iw / Self::value_width(value, unit).max(0.01));
        let mut y = PAD + size * CAP;
        self.value(
            self.w / 2.0,
            y,
            Anchor::Middle,
            size,
            Figure::plain(value, unit),
        );
        if let Some(text) = label {
            let (text, font) = self.name(text, 18.0, iw);
            y += 10.0 * u + font.size * CAP;
            self.text(self.w / 2.0, y, Anchor::Middle, font, &text);
        }
        let chart = Chart {
            x: PAD,
            y: y + 12.0 * u,
            w: iw,
            h: (self.h - PAD - y - 12.0 * u).max(0.0),
        };
        self.chart_frame(chart);
        let accent = self.accent;
        self.series(chart, points, accent, true);
    }

    pub(super) fn readings(&mut self, items: &[Reading]) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        if items.is_empty() {
            return;
        }
        if self.wide() {
            let count = items.len() as f32;
            let gap = 14.0 * u;
            let column = (iw - gap * (count - 1.0)) / count;
            let label_size = 18.0;
            let label_font = Font::new(label_size * u, 600, self.accent);
            let largest = 50.0 * u;
            // Numbers share one size; a word fits its own column.
            let size = items
                .iter()
                .filter(|item| {
                    item.unit.is_some()
                        || item
                            .value
                            .starts_with(|c: char| c.is_ascii_digit() || c == '-')
                })
                .map(|item| {
                    largest.min(
                        column / Self::value_width(&item.value, item.unit.as_deref()).max(0.01),
                    )
                })
                .fold(largest, f32::min)
                .max(24.0 * u);
            let label_gap = 14.0 * u;
            let block = label_font.size * CAP + label_gap + size * CAP;
            let top = PAD + (ih - block) / 2.0;
            for (i, item) in items.iter().enumerate() {
                let center = PAD + column / 2.0 + (column + gap) * i as f32;
                let (label, font) = self.name(&item.label, label_size, column);
                self.text(center, top + font.size * CAP, Anchor::Middle, font, &label);
                let fitted = size
                    .min(column / Self::value_width(&item.value, item.unit.as_deref()).max(0.01));
                let value = ellipsize(&item.value, Font::new(fitted, 700, self.color), column);
                self.value(
                    center,
                    top + label_font.size * CAP + label_gap + size * CAP,
                    Anchor::Middle,
                    fitted,
                    Figure {
                        value: &value,
                        unit: item.unit.as_deref(),
                        on: item.on,
                    },
                );
                if i > 0 {
                    let x = center - column / 2.0 - gap / 2.0;
                    let _ = write!(
                        self.svg,
                        r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" stroke="{}" stroke-opacity="0.18" stroke-width="1"/>"#,
                        PAD + ih * 0.15,
                        PAD + ih * 0.85,
                        self.color.hex(),
                    );
                }
            }
            return;
        }
        let rows = &items[..items.len().min(2)];
        let row = ih / rows.len() as f32;
        for (i, item) in rows.iter().enumerate() {
            let top = PAD + row * i as f32;
            let (label, font) = self.name(&item.label, 18.0, iw);
            let size = (40.0 * u)
                .min(iw / Self::value_width(&item.value, item.unit.as_deref()).max(0.01))
                .min((row - font.size * CAP - 14.0 * u) / CAP);
            let block = font.size * CAP + 8.0 * u + size * CAP;
            let y = top + (row - block) / 2.0 + font.size * CAP;
            self.text(self.w / 2.0, y, Anchor::Middle, font, &label);
            let value = ellipsize(&item.value, Font::new(size, 700, self.color), iw);
            self.value(
                self.w / 2.0,
                y + 8.0 * u + size * CAP,
                Anchor::Middle,
                size,
                Figure {
                    value: &value,
                    unit: item.unit.as_deref(),
                    on: item.on,
                },
            );
        }
    }

    pub(super) fn picture(&mut self, image: Option<&ImageData>, missing: &str) {
        match image {
            Some(image) => {
                let area = Chart {
                    x: 0.0,
                    y: 0.0,
                    w: self.w,
                    h: self.h,
                };
                self.image(image, area, "xMidYMid slice", None);
            }
            None => {
                let (iw, _) = self.inner();
                let font = Font::new(22.0 * self.unit, 600, self.color)
                    .dim(0.6)
                    .fit(missing, iw);
                self.text(
                    self.w / 2.0,
                    self.h / 2.0 + font.size * CAP / 2.0,
                    Anchor::Middle,
                    font,
                    missing,
                );
            }
        }
    }

    pub(super) fn weather(
        &mut self,
        condition: &str,
        text: &str,
        temperature: &str,
        detail: Option<&str>,
        days: &[WeatherDay],
    ) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        if !self.wide() {
            let glyph = ih * 0.42;
            let size = (44.0 * u).min(iw / Self::value_width(temperature, None).max(0.01));
            let (text, text_font) = Self::fitted(
                text,
                Font::new(19.0 * u, 600, self.color).dim(0.75),
                iw,
                17.0 * u,
            );
            let block = glyph + 10.0 * u + size * CAP + 10.0 * u + text_font.size * CAP;
            let mut y = PAD + (ih - block) / 2.0;
            self.weather_glyph(self.w / 2.0, y + glyph / 2.0, glyph, condition);
            y += glyph + 10.0 * u + size * CAP;
            self.value(
                self.w / 2.0,
                y,
                Anchor::Middle,
                size,
                Figure::plain(temperature, None),
            );
            y += 10.0 * u + text_font.size * CAP;
            self.text(self.w / 2.0, y, Anchor::Middle, text_font, &text);
            return;
        }
        let days = &days[..days.len().min(3)];
        let now_width = if days.is_empty() { iw } else { iw * 0.5 };
        let glyph = (ih * 0.56).min(now_width * 0.34);
        if !condition.is_empty() {
            self.weather_glyph(PAD + glyph / 2.0, PAD + ih / 2.0, glyph, condition);
        }
        let left = PAD + glyph + 14.0 * u;
        let room = now_width - glyph - 14.0 * u;
        let size = (56.0 * u).min(room / Self::value_width(temperature, None).max(0.01));
        let (text, text_font) = Self::fitted(
            text,
            Font::new(21.0 * u, 600, self.color).dim(0.75),
            room,
            17.0 * u,
        );
        let detail = detail.and_then(|detail| {
            let font = Font::new(18.0 * u, 600, self.color)
                .dim(0.6)
                .fit(detail, room);
            (font.size >= 17.0 * u).then_some((detail, font))
        });
        let gap = 10.0 * u;
        let block = size * CAP
            + gap
            + text_font.size * CAP
            + detail.map_or(0.0, |(_, font)| gap + font.size * CAP);
        let mut y = PAD + (ih - block) / 2.0 + size * CAP;
        self.value(
            left,
            y,
            Anchor::Start,
            size,
            Figure::plain(temperature, None),
        );
        y += gap + text_font.size * CAP;
        self.text(left, y, Anchor::Start, text_font, &text);
        if let Some((detail, font)) = detail {
            y += gap + font.size * CAP;
            self.text(left, y, Anchor::Start, font, detail);
        }
        if days.is_empty() {
            return;
        }
        let start = PAD + now_width + 10.0 * u;
        let width = self.w - PAD - start;
        let column = width / days.len() as f32;
        let name_font = Font::new(17.0 * u, 600, self.accent);
        let high_font = Font::new(22.0 * u, 700, self.color);
        let low_font = Font::new(18.0 * u, 600, self.color).dim(0.6);
        let small = (column * 0.62).min(ih * 0.3);
        let block = name_font.size * CAP
            + 10.0 * u
            + small
            + 10.0 * u
            + high_font.size * CAP
            + 8.0 * u
            + low_font.size * CAP;
        let top = PAD + (ih - block) / 2.0;
        for (i, day) in days.iter().enumerate() {
            let center = start + column * (i as f32 + 0.5);
            let mut y = top + name_font.size * CAP;
            let font = name_font.fit(&day.name, column - 4.0 * u);
            self.text(center, y, Anchor::Middle, font, &day.name);
            y += 10.0 * u;
            self.weather_glyph(center, y + small / 2.0, small, &day.condition);
            y += small + 10.0 * u + high_font.size * CAP;
            if let Some(high) = &day.high {
                self.text(center, y, Anchor::Middle, high_font.fit(high, column), high);
            }
            y += 8.0 * u + low_font.size * CAP;
            if let Some(low) = &day.low {
                self.text(center, y, Anchor::Middle, low_font.fit(low, column), low);
            }
        }
    }

    /// Home Assistant's weather conditions, drawn in a box `s` wide centered at `cx`, `cy`.
    fn weather_glyph(&mut self, cx: f32, cy: f32, s: f32, condition: &str) {
        let precipitation = |pen: &mut Self, kind: &str| {
            pen.cloud(cx, cy - s * 0.1, s * 0.86);
            let y = cy + s * 0.2;
            match kind {
                "rain" => pen.drops(cx, y, s, 3),
                "pour" => pen.drops(cx, y, s, 5),
                "snow" => pen.flakes(cx, y, s, 3),
                "sleet" => {
                    pen.drops(cx - s * 0.12, y, s * 0.7, 2);
                    pen.flakes(cx + s * 0.18, y, s * 0.6, 1);
                }
                "hail" => pen.flakes(cx, y, s * 1.1, 4),
                "bolt" => pen.bolt(cx, y, s),
                "storm" => {
                    pen.bolt(cx + s * 0.1, y, s * 0.9);
                    pen.drops(cx - s * 0.2, y, s * 0.6, 2);
                }
                _ => {}
            }
        };
        match condition {
            "sunny" => self.sun(cx, cy, s * 0.24),
            "clear-night" => self.crescent(cx, cy, s * 0.34),
            "partlycloudy" => {
                self.sun(cx - s * 0.16, cy - s * 0.14, s * 0.17);
                self.cloud(cx + s * 0.06, cy + s * 0.08, s * 0.8);
            }
            "cloudy" => self.cloud(cx, cy, s),
            "rainy" => precipitation(self, "rain"),
            "pouring" => precipitation(self, "pour"),
            "snowy" => precipitation(self, "snow"),
            "snowy-rainy" => precipitation(self, "sleet"),
            "hail" => precipitation(self, "hail"),
            "lightning" => precipitation(self, "bolt"),
            "lightning-rainy" => precipitation(self, "storm"),
            "fog" => {
                self.cloud(cx, cy - s * 0.12, s * 0.8);
                self.lines_across(cx, cy + s * 0.16, s, 3);
            }
            "windy" => self.wind(cx, cy, s),
            "windy-variant" => {
                self.cloud(cx, cy - s * 0.12, s * 0.8);
                self.wind(cx, cy + s * 0.2, s * 0.7);
            }
            "exceptional" => {
                let _ = write!(
                    self.svg,
                    r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="none" stroke="{}" stroke-width="{}"/>"#,
                    s * 0.36,
                    self.accent.hex(),
                    s * 0.07,
                );
                let font = Font::new(s * 0.5, 800, self.accent);
                self.text(cx, cy + s * 0.5 * CAP / 2.0, Anchor::Middle, font, "!");
            }
            _ => self.cloud(cx, cy, s),
        }
    }

    fn sun(&mut self, cx: f32, cy: f32, r: f32) {
        let hex = self.accent.hex();
        let _ = write!(
            self.svg,
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{hex}"/>"#
        );
        for i in 0..8 {
            let angle = i as f32 * PI / 4.0;
            let (sin, cos) = angle.sin_cos();
            let _ = write!(
                self.svg,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{hex}" stroke-width="{}" stroke-linecap="round"/>"#,
                cx + cos * r * 1.4,
                cy + sin * r * 1.4,
                cx + cos * r * 1.85,
                cy + sin * r * 1.85,
                r * 0.24,
            );
        }
    }

    fn crescent(&mut self, cx: f32, cy: f32, r: f32) {
        let id = format!("crescent{}x{}", cx.round() as i32, cy.round() as i32);
        let _ = write!(
            self.svg,
            r#"<mask id="{id}"><rect x="{}" y="{}" width="{}" height="{}" fill="white"/><circle cx="{}" cy="{}" r="{}" fill="black"/></mask><circle cx="{cx}" cy="{cy}" r="{r}" fill="{}" mask="url(#{id})"/>"#,
            cx - r * 2.0,
            cy - r * 2.0,
            r * 4.0,
            r * 4.0,
            cx + r * 0.5,
            cy - r * 0.35,
            r * 0.82,
            self.accent.hex(),
        );
    }

    fn cloud(&mut self, cx: f32, cy: f32, s: f32) {
        let hex = self.color.hex();
        let _ = write!(
            self.svg,
            r#"<g fill="{hex}"><circle cx="{}" cy="{}" r="{}"/><circle cx="{}" cy="{}" r="{}"/><circle cx="{}" cy="{}" r="{}"/><rect x="{}" y="{}" width="{}" height="{}" rx="{}"/></g>"#,
            cx - s * 0.05,
            cy - s * 0.06,
            s * 0.22,
            cx - s * 0.26,
            cy + s * 0.06,
            s * 0.14,
            cx + s * 0.22,
            cy + s * 0.04,
            s * 0.16,
            cx - s * 0.4,
            cy + s * 0.04,
            s * 0.8,
            s * 0.16,
            s * 0.08,
        );
    }

    fn drops(&mut self, cx: f32, y: f32, s: f32, count: usize) {
        let hex = self.accent.hex();
        let spacing = s * 0.16;
        let first = cx - spacing * (count as f32 - 1.0) / 2.0;
        for i in 0..count {
            let x = first + spacing * i as f32;
            let _ = write!(
                self.svg,
                r#"<line x1="{x}" y1="{y}" x2="{}" y2="{}" stroke="{hex}" stroke-width="{}" stroke-linecap="round"/>"#,
                x - s * 0.05,
                y + s * 0.16,
                s * 0.05,
            );
        }
    }

    fn flakes(&mut self, cx: f32, y: f32, s: f32, count: usize) {
        let hex = self.color.hex();
        let spacing = s * 0.16;
        let first = cx - spacing * (count as f32 - 1.0) / 2.0;
        for i in 0..count {
            let x = first + spacing * i as f32;
            let _ = write!(
                self.svg,
                r#"<circle cx="{x}" cy="{}" r="{}" fill="{hex}"/>"#,
                y + s * 0.08 + if i % 2 == 0 { 0.0 } else { s * 0.06 },
                s * 0.04,
            );
        }
    }

    fn bolt(&mut self, cx: f32, y: f32, s: f32) {
        let _ = write!(
            self.svg,
            r#"<polygon points="{},{} {},{} {},{} {},{} {},{} {},{}" fill="{}"/>"#,
            cx + s * 0.04,
            y - s * 0.02,
            cx - s * 0.1,
            y + s * 0.13,
            cx - s * 0.01,
            y + s * 0.13,
            cx - s * 0.06,
            y + s * 0.28,
            cx + s * 0.1,
            y + s * 0.08,
            cx + s * 0.01,
            y + s * 0.08,
            self.accent.hex(),
        );
    }

    fn lines_across(&mut self, cx: f32, y: f32, s: f32, count: usize) {
        let hex = self.color.hex();
        for i in 0..count {
            let indent = if i % 2 == 0 { 0.0 } else { s * 0.08 };
            let row = y + s * 0.1 * i as f32;
            let _ = write!(
                self.svg,
                r#"<line x1="{}" y1="{row}" x2="{}" y2="{row}" stroke="{hex}" stroke-opacity="0.7" stroke-width="{}" stroke-linecap="round"/>"#,
                cx - s * 0.36 + indent,
                cx + s * 0.36 - indent,
                s * 0.05,
            );
        }
    }

    fn wind(&mut self, cx: f32, cy: f32, s: f32) {
        let hex = self.color.hex();
        for (dy, length) in [(-0.14, 0.7), (0.0, 0.85), (0.14, 0.55)] {
            let y = cy + s * dy;
            let start = cx - s * 0.42;
            let end = start + s * length;
            let _ = write!(
                self.svg,
                r#"<path d="M{start} {y}L{end} {y}q{} 0 {} {}" fill="none" stroke="{hex}" stroke-width="{}" stroke-linecap="round"/>"#,
                s * 0.08,
                s * 0.08,
                -s * 0.08,
                s * 0.05,
            );
        }
    }
}
