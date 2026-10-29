//! Draws live widgets as SVG. Layouts are made for the wide key (452×196) and fall back to a
//! stacked layout on square keys. Without font metrics here, text widths are estimated, with
//! room to spare, to pick sizes that fit.

use std::fmt::Write;

use super::design::{
    ClockFace, Color, Meter, NetworkGraph, Rate, TimerPhase, WidgetContent, WidgetView,
};
use super::hass_drawings::Tile;
use super::hass_widgets::{Card, Figure};
use super::svg::{SAFE_MARGIN, escape};

/// Room inside the safe margin, so nothing touches the bezel.
pub(super) const PAD: f32 = SAFE_MARGIN + 8.0;
/// Height of digits and capitals, and depth of descenders, as a fraction of the font size.
pub(super) const CAP: f32 = 0.73;
const DESCENT: f32 = 0.22;
/// Keys at least this much wider than tall lay their parts side by side.
const WIDE: f32 = 1.6;
/// Height the sizes below are designed for.
const DESIGN_HEIGHT: f32 = 196.0;

pub fn write(svg: &mut String, view: &WidgetView, w: f32, h: f32) {
    let mut pen = Pen {
        svg,
        family: escape(&view.font_family),
        color: view.color,
        accent: view.accent,
        w,
        h,
        unit: h / DESIGN_HEIGHT,
    };
    match &view.content {
        WidgetContent::DigitalClock {
            time,
            seconds,
            suffix,
            label,
            line,
        } => pen.digital_clock(
            time,
            seconds.as_deref(),
            suffix.as_deref(),
            label.as_deref(),
            line.as_deref(),
        ),
        WidgetContent::AnalogClock {
            hour,
            minute,
            second,
            label,
            weekday,
            date,
        } => pen.analog_clock(
            Hands {
                hour: *hour,
                minute: *minute,
                second: *second,
            },
            label.as_deref(),
            weekday.as_deref(),
            date.as_deref(),
        ),
        WidgetContent::Meters {
            meters,
            graph: false,
        } => pen.meters(meters),
        WidgetContent::Meters {
            meters,
            graph: true,
        } => pen.meter_graphs(meters),
        WidgetContent::Network {
            down,
            up,
            graph: None,
        } => pen.network(down, up),
        WidgetContent::Network {
            down,
            up,
            graph: Some(graph),
        } => pen.network_graph(down, up, graph),
        WidgetContent::Countdown {
            label,
            main,
            detail,
        } => pen.countdown(label.as_deref(), main, detail.as_deref()),
        WidgetContent::WorldClocks { clocks } => pen.world_clocks(clocks),
        WidgetContent::Calendar {
            weekday,
            day,
            month,
            weeks,
            today,
        } => pen.calendar(weekday, day, month, weeks, *today),
        WidgetContent::Timer {
            label,
            main,
            progress,
            phase,
        } => pen.timer(label.as_deref(), main, *progress, *phase),
        WidgetContent::Battery {
            level,
            text,
            detail,
            charging,
            low,
        } => pen.battery(*level, text, detail, *charging, *low),
        WidgetContent::NowPlaying {
            title,
            artist,
            playing,
            progress,
            time,
        } => pen.now_playing(
            title.as_deref(),
            artist,
            *playing,
            *progress,
            time.as_deref(),
        ),
        WidgetContent::Latency {
            value,
            target,
            history,
            peak,
            ok,
        } => pen.latency(value, target, history, peak, *ok),
        WidgetContent::Moon {
            phase,
            name,
            detail,
        } => pen.moon(*phase, name, detail),
        WidgetContent::WordClock { lead, hour, rest } => pen.word_clock(lead, hour, rest),
        WidgetContent::Phrase { text } => pen.phrase(text),
        WidgetContent::Entity {
            label,
            value,
            unit,
            detail,
            extra,
            level,
            on,
            tint,
            icon,
        } => pen.entity(&Card {
            label: label.as_deref(),
            figure: Figure {
                value,
                unit: unit.as_deref(),
                on: *on,
            },
            detail: detail.as_deref(),
            extra: extra.as_deref(),
            level: *level,
            tint: *tint,
            icon: icon.as_ref(),
        }),
        WidgetContent::History {
            label,
            value,
            unit,
            points,
            low,
            high,
        } => pen.history(label.as_deref(), value, unit.as_deref(), points, low, high),
        WidgetContent::Readings { items } => pen.readings(items),
        WidgetContent::Picture { image, missing } => pen.picture(image.as_ref(), missing),
        WidgetContent::Weather {
            condition,
            text,
            temperature,
            detail,
            days,
        } => pen.weather(condition, text, temperature, detail.as_deref(), days),
        WidgetContent::Tile {
            label,
            value,
            unit,
            detail,
            extra,
            badge,
            on,
            tint,
            drawing,
        } => pen.tile(&Tile {
            label: label.as_deref(),
            figure: Figure {
                value,
                unit: unit.as_deref(),
                on: *on,
            },
            detail: detail.as_deref(),
            extra: extra.as_deref(),
            badge: badge.as_deref(),
            tint: *tint,
            drawing,
        }),
    }
}

/// Words of `text` in lines no wider than `width`.
pub(super) fn wrap(text: &str, font: Font, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if text_width(&format!("{line} {word}"), font) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

pub(super) const RED: Color = Color::rgb(239, 68, 68);

fn week_letters() -> [&'static str; 7] {
    [
        tr!("widgets.calendar.initials.monday"),
        tr!("widgets.calendar.initials.tuesday"),
        tr!("widgets.calendar.initials.wednesday"),
        tr!("widgets.calendar.initials.thursday"),
        tr!("widgets.calendar.initials.friday"),
        tr!("widgets.calendar.initials.saturday"),
        tr!("widgets.calendar.initials.sunday"),
    ]
}

/// `text` cut with an ellipsis to fit in `width`.
pub(super) fn ellipsize(text: &str, font: Font, width: f32) -> String {
    if text_width(text, font) <= width {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut: String = chars.iter().collect::<String>().trim_end().to_owned() + "…";
        if text_width(&cut, font) <= width {
            return cut;
        }
    }
    "…".into()
}

#[derive(Clone, Copy)]
pub(super) enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }

    /// Where a run of `width` starts when anchored at `x`.
    pub(super) fn start(self, x: f32, width: f32) -> f32 {
        match self {
            Self::Start => x,
            Self::Middle => x - width / 2.0,
            Self::End => x - width,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Font {
    pub(super) size: f32,
    weight: u16,
    fill: Color,
    opacity: f32,
    /// Extra space between letters, in ems.
    spacing: f32,
}

impl Font {
    pub(super) fn new(size: f32, weight: u16, fill: Color) -> Self {
        Self {
            size,
            weight,
            fill,
            opacity: 1.0,
            spacing: 0.0,
        }
    }

    pub(super) fn dim(self, opacity: f32) -> Self {
        Self { opacity, ..self }
    }

    pub(super) fn spaced(self, spacing: f32) -> Self {
        Self { spacing, ..self }
    }

    pub(super) fn sized(self, size: f32) -> Self {
        Self { size, ..self }
    }

    /// Shrinks the font until `text` fits in `width`.
    pub(super) fn fit(self, text: &str, width: f32) -> Self {
        let natural = text_width(text, self);
        if natural <= width || natural <= 0.0 {
            self
        } else {
            self.sized(self.size * width / natural)
        }
    }

    pub(super) fn attributes(self, family: &str) -> String {
        let mut out = format!(
            r#"font-family="{family}" font-size="{}" font-weight="{}" fill="{}""#,
            self.size,
            self.weight,
            self.fill.hex()
        );
        if self.opacity < 1.0 {
            let _ = write!(out, r#" fill-opacity="{}""#, self.opacity);
        }
        if self.spacing != 0.0 {
            let _ = write!(out, r#" letter-spacing="{}""#, self.spacing * self.size);
        }
        out
    }
}

/// Rough advance of a character in ems, close to Inter and on the wide side for most fonts.
fn advance(c: char) -> f32 {
    match c {
        'i' | 'l' | 'j' | 'í' | 'ï' | 'I' | '.' | ',' | ':' | ';' | '!' | '¡' | '\'' | '|' => {
            0.28
        }
        ' ' => 0.28,
        'f' | 't' | 'r' => 0.38,
        'm' | 'w' => 0.86,
        'M' | 'W' => 0.94,
        '%' => 0.82,
        '0'..='9' => 0.64,
        c if c.is_uppercase() => 0.7,
        _ => 0.57,
    }
}

pub(super) fn text_width(text: &str, font: Font) -> f32 {
    let bold = 1.0 + f32::from(font.weight.saturating_sub(400)) / 3000.0;
    let count = text.chars().count() as f32;
    text.chars().map(advance).sum::<f32>() * font.size * bold
        + font.spacing * font.size * (count - 1.0).max(0.0)
}

/// Width of a character's cell when digits must not move as they change.
fn cell(c: char) -> f32 {
    match c {
        '0'..='9' => 0.64,
        ':' | '.' | ',' | ' ' => 0.32,
        _ => advance(c),
    }
}

pub(super) fn tabular_width(text: &str, size: f32) -> f32 {
    text.chars().map(cell).sum::<f32>() * size
}

#[derive(Clone, Copy)]
pub(super) struct Chart {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

struct Hands {
    hour: f32,
    minute: f32,
    second: Option<f32>,
}

pub(super) struct Pen<'a> {
    pub(super) svg: &'a mut String,
    pub(super) family: String,
    pub(super) color: Color,
    pub(super) accent: Color,
    pub(super) w: f32,
    pub(super) h: f32,
    /// Scale of the designed sizes for this key.
    pub(super) unit: f32,
}

impl Pen<'_> {
    pub(super) fn inner(&self) -> (f32, f32) {
        (self.w - 2.0 * PAD, self.h - 2.0 * PAD)
    }

    pub(super) fn wide(&self) -> bool {
        self.w >= WIDE * self.h
    }

    pub(super) fn text(&mut self, x: f32, y: f32, anchor: Anchor, font: Font, content: &str) {
        let _ = write!(
            self.svg,
            r#"<text x="{x}" y="{y}" text-anchor="{}" {}>{}</text>"#,
            anchor.name(),
            font.attributes(&self.family),
            escape(content),
        );
    }

    /// Each character centered in a cell of fixed width, so a clock does not wobble as it ticks.
    pub(super) fn tabular(&mut self, x: f32, y: f32, anchor: Anchor, font: Font, content: &str) {
        let width = tabular_width(content, font.size);
        let mut left = anchor.start(x, width);
        let _ = write!(
            self.svg,
            r#"<g text-anchor="middle" {}>"#,
            font.attributes(&self.family)
        );
        for c in content.chars() {
            let span = cell(c) * font.size;
            // Colons sit low next to digits; lift them to the middle of the figures.
            let lift = if c == ':' { font.size * 0.06 } else { 0.0 };
            let mut glyph = [0; 4];
            let _ = write!(
                self.svg,
                r#"<text x="{}" y="{}">{}</text>"#,
                left + span / 2.0,
                y - lift,
                escape(c.encode_utf8(&mut glyph)),
            );
            left += span;
        }
        self.svg.push_str("</g>");
    }

    pub(super) fn label_font(&self) -> Font {
        Font::new(17.0 * self.unit, 600, self.accent).spaced(0.14)
    }

    fn digital_clock(
        &mut self,
        time: &str,
        seconds: Option<&str>,
        suffix: Option<&str>,
        label: Option<&str>,
        line: Option<&str>,
    ) {
        const SECONDS: f32 = 0.42;
        const SUFFIX: f32 = 0.26;
        const COLUMN_GAP: f32 = 0.06;
        let (iw, ih) = self.inner();
        let u = self.unit;
        let label = label.map(|text| (text.to_uppercase(), self.label_font()));
        let label = label.map(|(text, font)| {
            let font = font.fit(&text, iw);
            (text, font)
        });
        let line = line.map(|text| {
            (
                text,
                Font::new(25.0 * u, 500, self.color).dim(0.72).fit(text, iw),
            )
        });
        let color = self.color;
        let suffix_font = move |size: f32| Font::new(size * SUFFIX, 600, color).dim(0.72);

        let column = seconds
            .map(|s| tabular_width(s, SECONDS))
            .into_iter()
            .chain(suffix.map(|s| text_width(s, suffix_font(1.0))))
            .fold(0.0, f32::max);
        let width = tabular_width(time, 1.0)
            + if column > 0.0 {
                COLUMN_GAP + column
            } else {
                0.0
            };
        let label_gap = 14.0 * u;
        let line_gap = 18.0 * u;
        let above = label
            .as_ref()
            .map_or(0.0, |(_, f)| f.size * CAP + label_gap);
        let below = line
            .as_ref()
            .map_or(0.0, |(_, f)| line_gap + f.size * (CAP + DESCENT / 2.0));
        let preferred = if label.is_none() && line.is_none() {
            118.0
        } else {
            100.0
        } * u;
        let size = preferred.min(iw / width).min((ih - above - below) / CAP);
        let total = above + size * CAP + below;

        let center = self.w / 2.0;
        let mut y = PAD + (ih - total) / 2.0;
        if let Some((text, font)) = &label {
            y += font.size * CAP;
            self.text(center, y, Anchor::Middle, *font, text);
            y += label_gap;
        }
        y += size * CAP;
        let baseline = y;
        let time_width = tabular_width(time, size);
        let left = center - width * size / 2.0;
        self.tabular(
            left,
            baseline,
            Anchor::Start,
            Font::new(size, 700, self.color),
            time,
        );
        let column_x = left + time_width + COLUMN_GAP * size;
        if let Some(seconds) = seconds {
            self.tabular(
                column_x,
                baseline,
                Anchor::Start,
                Font::new(size * SECONDS, 600, self.accent),
                seconds,
            );
        }
        if let Some(suffix) = suffix {
            let font = suffix_font(size);
            let top = baseline - size * CAP + font.size * CAP;
            self.text(column_x, top, Anchor::Start, font, suffix);
        }
        if let Some((text, font)) = &line {
            y += line_gap + font.size * CAP;
            self.text(center, y, Anchor::Middle, *font, text);
        }
    }

    fn analog_clock(
        &mut self,
        hands: Hands,
        label: Option<&str>,
        weekday: Option<&str>,
        date: Option<&str>,
    ) {
        let pad = SAFE_MARGIN + 4.0;
        let (iw, ih) = (self.w - 2.0 * pad, self.h - 2.0 * pad);
        let u = self.unit;
        let diameter = iw.min(ih);
        let radius = diameter / 2.0;
        let side = self.wide() && (label.is_some() || weekday.is_some() || date.is_some());
        let cy = self.h / 2.0;
        if !side {
            self.dial(self.w / 2.0, cy, radius, &hands, label);
            return;
        }

        let gap = 28.0 * u;
        let room = iw - diameter - gap;
        let label = label.map(|text| {
            let text = text.to_uppercase();
            let font = self.label_font().fit(&text, room);
            (text, font)
        });
        let weekday =
            weekday.map(|text| (text, Font::new(38.0 * u, 700, self.color).fit(text, room)));
        let date = date.map(|text| {
            (
                text,
                Font::new(25.0 * u, 500, self.color)
                    .dim(0.72)
                    .fit(text, room),
            )
        });
        let text_width = [
            label.as_ref().map(|(t, f)| text_width(t, *f)),
            weekday.as_ref().map(|(t, f)| text_width(t, *f)),
            date.as_ref().map(|(t, f)| text_width(t, *f)),
        ]
        .into_iter()
        .flatten()
        .fold(0.0, f32::max);
        let group = diameter + gap + text_width.min(room);
        let left = pad + ((iw - group) / 2.0).max(0.0);
        self.dial(left + radius, cy, radius, &hands, None);

        let x = left + diameter + gap;
        let line_gap = 12.0 * u;
        let parts: Vec<(&str, Font)> =
            [label.as_ref().map(|(t, f)| (t.as_str(), *f)), weekday, date]
                .into_iter()
                .flatten()
                .collect();
        let total = parts.iter().map(|(_, f)| f.size * CAP).sum::<f32>()
            + line_gap * (parts.len() as f32 - 1.0);
        let mut y = cy - total / 2.0;
        for (text, font) in parts {
            y += font.size * CAP;
            self.text(x, y, Anchor::Start, font, text);
            y += line_gap;
        }
    }

    fn dial(&mut self, cx: f32, cy: f32, r: f32, hands: &Hands, label: Option<&str>) {
        let color = self.color.hex();
        let accent = self.accent.hex();
        let _ = write!(
            self.svg,
            r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="{color}" fill-opacity="0.06" stroke="{color}" stroke-opacity="0.22" stroke-width="{}"/>"#,
            r - 1.0,
            (r * 0.025).max(1.5),
        );
        for i in 0..60 {
            let quarter = i % 15 == 0;
            let hour = i % 5 == 0;
            let (length, width, opacity) = if quarter {
                (0.16, 0.05, 1.0)
            } else if hour {
                (0.1, 0.03, 0.75)
            } else {
                (0.035, 0.015, 0.35)
            };
            let outer = r * 0.9;
            let _ = write!(
                self.svg,
                r#"<line x1="{cx}" y1="{}" x2="{cx}" y2="{}" stroke="{color}" stroke-opacity="{opacity}" stroke-width="{}" stroke-linecap="round" transform="rotate({} {cx} {cy})"/>"#,
                cy - outer,
                cy - outer + r * length,
                (r * width).max(1.0),
                i * 6,
            );
        }
        if let Some(label) = label {
            let text = label.to_uppercase();
            let font = Font::new(r * 0.13, 600, self.accent)
                .spaced(0.12)
                .fit(&text, r);
            self.text(
                cx,
                cy + r * 0.36 + font.size * CAP,
                Anchor::Middle,
                font,
                &text,
            );
        }
        let mut hand = |angle: f32, length: f32, back: f32, width: f32, paint: &str| {
            let _ = write!(
                self.svg,
                r#"<line x1="{cx}" y1="{}" x2="{cx}" y2="{}" stroke="{paint}" stroke-width="{}" stroke-linecap="round" transform="rotate({angle} {cx} {cy})"/>"#,
                cy + r * back,
                cy - r * length,
                (r * width).max(1.5),
            );
        };
        hand(hands.hour, 0.5, 0.08, 0.075, &color);
        hand(hands.minute, 0.76, 0.08, 0.05, &color);
        match hands.second {
            Some(angle) => {
                hand(angle, 0.84, 0.18, 0.022, &accent);
                let _ = write!(
                    self.svg,
                    r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="{accent}"/>"#,
                    r * 0.055
                );
            }
            None => {
                let _ = write!(
                    self.svg,
                    r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="{color}"/>"#,
                    r * 0.055
                );
            }
        }
    }

    fn meters(&mut self, meters: &[Meter]) {
        if meters.is_empty() {
            return;
        }
        let (iw, ih) = self.inner();
        let u = self.unit;
        let count = meters.len() as f32;
        let row = (ih / count).min(58.0 * u);
        let top = PAD + (ih - row * count) / 2.0;
        let track = self.color.hex();
        let accent = self.accent.hex();
        let wide = self.wide();
        let size = if wide {
            (row * 0.46).min(26.0 * u)
        } else {
            (row * 0.36).min(22.0 * u)
        };
        let label_font = Font::new(size, 600, self.color).dim(0.72).spaced(0.04);
        let value_font = Font::new(size, 700, self.color);
        let label_width = meters
            .iter()
            .map(|m| text_width(&m.label, label_font))
            .fold(0.0, f32::max);
        let value_width = text_width("100 %", value_font);

        for (i, meter) in meters.iter().enumerate() {
            let row_top = top + row * i as f32;
            let (bar_x, bar_width, bar_y, bar_height, baseline) = if wide {
                let bar_height = (row * 0.26).clamp(8.0 * u, 16.0 * u);
                let baseline = row_top + (row + size * CAP) / 2.0;
                let bar_x = PAD + label_width + 16.0 * u;
                let bar_width = iw - label_width - value_width - 28.0 * u;
                let bar_y = row_top + (row - bar_height) / 2.0;
                (bar_x, bar_width, bar_y, bar_height, baseline)
            } else {
                let bar_height = (row * 0.16).clamp(5.0 * u, 10.0 * u);
                let block = size * CAP + 8.0 * u + bar_height;
                let baseline = row_top + (row - block) / 2.0 + size * CAP;
                (PAD, iw, baseline + 8.0 * u, bar_height, baseline)
            };
            self.text(PAD, baseline, Anchor::Start, label_font, &meter.label);
            self.text(self.w - PAD, baseline, Anchor::End, value_font, &meter.text);
            let radius = bar_height / 2.0;
            let _ = write!(
                self.svg,
                r#"<rect x="{bar_x}" y="{bar_y}" width="{bar_width}" height="{bar_height}" rx="{radius}" fill="{track}" fill-opacity="0.14"/>"#,
            );
            if let Some(level) = meter.level {
                let filled = bar_width * level.clamp(0.0, 1.0);
                if filled > 0.0 {
                    let _ = write!(
                        self.svg,
                        r#"<rect x="{bar_x}" y="{bar_y}" width="{}" height="{bar_height}" rx="{radius}" fill="{accent}"/>"#,
                        filled.max(bar_height),
                    );
                }
            }
        }
    }

    /// Each meter's last minutes as an area, in the place of its bar.
    fn meter_graphs(&mut self, meters: &[Meter]) {
        if meters.is_empty() {
            return;
        }
        let (iw, ih) = self.inner();
        let u = self.unit;
        if let [meter] = meters
            && self.wide()
        {
            let column = iw * 0.34;
            let label_font = self.label_font();
            let value_font = Font::new(64.0 * u, 700, self.color).fit(&meter.text, column);
            let block = label_font.size * CAP + 16.0 * u + value_font.size * CAP;
            let top = PAD + (ih - block) / 2.0;
            self.text(
                PAD,
                top + label_font.size * CAP,
                Anchor::Start,
                label_font,
                &meter.label.to_uppercase(),
            );
            self.tabular(PAD, top + block, Anchor::Start, value_font, &meter.text);
            let chart = Chart {
                x: PAD + column + 12.0 * u,
                y: PAD,
                w: iw - column - 12.0 * u,
                h: ih,
            };
            self.chart_frame(chart);
            self.series(chart, &meter.history, self.accent, true);
            return;
        }
        let count = meters.len() as f32;
        let gap = 8.0 * u;
        let row = (ih - gap * (count - 1.0)) / count;
        let wide = self.wide();
        let size = if wide {
            (row * 0.4).min(24.0 * u)
        } else {
            (row * 0.3).min(20.0 * u)
        };
        let label_font = Font::new(size, 600, self.color).dim(0.72).spaced(0.04);
        let value_font = Font::new(size, 700, self.color);
        let label_width = meters
            .iter()
            .map(|m| text_width(&m.label, label_font))
            .fold(0.0, f32::max);
        let value_width = text_width("100 %", value_font);

        for (i, meter) in meters.iter().enumerate() {
            let row_top = PAD + (row + gap) * i as f32;
            let (chart, baseline) = if wide {
                let x = PAD + label_width + 14.0 * u;
                let chart = Chart {
                    x,
                    y: row_top,
                    w: iw - label_width - value_width - 26.0 * u,
                    h: row,
                };
                (chart, row_top + (row + size * CAP) / 2.0)
            } else {
                let baseline = row_top + size * CAP;
                let top = baseline + 6.0 * u;
                let chart = Chart {
                    x: PAD,
                    y: top,
                    w: iw,
                    h: row_top + row - top,
                };
                (chart, baseline)
            };
            self.text(PAD, baseline, Anchor::Start, label_font, &meter.label);
            self.text(self.w - PAD, baseline, Anchor::End, value_font, &meter.text);
            self.chart_frame(chart);
            self.series(chart, &meter.history, self.accent, true);
        }
    }

    /// The speeds now beside both speeds over the last minutes, against the fastest one.
    fn network_graph(&mut self, down: &Rate, up: &Rate, graph: &NetworkGraph) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let peak_font = Font::new(13.0 * u, 600, self.color).dim(0.5);
        let peak = format!("{} {}", graph.peak.value, graph.peak.unit);
        if self.wide() {
            let column = iw * 0.4;
            let row = ih / 2.0;
            let arrow = 22.0 * u;
            let room = column - arrow - 10.0 * u;
            for (i, (rate, is_down)) in [(down, true), (up, false)].into_iter().enumerate() {
                let width = tabular_width(&rate.value, 1.0)
                    + 0.12
                    + text_width(&rate.unit, Font::new(0.42, 600, self.color));
                let size = (40.0 * u).min(room / width).min(row * 0.62 / CAP);
                let baseline = PAD + row * i as f32 + (row + size * CAP) / 2.0;
                self.arrow_in(
                    PAD + arrow / 2.0,
                    baseline - size * CAP / 2.0,
                    arrow,
                    is_down,
                    if is_down { self.accent } else { self.color },
                );
                self.rate(PAD + arrow + 10.0 * u, baseline, size, rate, Anchor::Start);
            }
            let chart = Chart {
                x: PAD + column + 12.0 * u,
                y: PAD + peak_font.size * CAP + 6.0 * u,
                w: iw - column - 12.0 * u,
                h: ih - peak_font.size * CAP - 6.0 * u,
            };
            self.text(
                chart.x + chart.w,
                PAD + peak_font.size * CAP,
                Anchor::End,
                peak_font,
                &peak,
            );
            self.chart_frame(chart);
            self.series(chart, &graph.down, self.accent, true);
            self.series(chart, &graph.up, self.color, false);
        } else {
            let size = 22.0 * u;
            let half = iw / 2.0;
            let arrow = 16.0 * u;
            let baseline = PAD + size * CAP;
            for (i, (rate, is_down)) in [(down, true), (up, false)].into_iter().enumerate() {
                let left = PAD + half * i as f32;
                let width = tabular_width(&rate.value, 1.0)
                    + 0.12
                    + text_width(&rate.unit, Font::new(0.42, 600, self.color));
                let size = size.min((half - arrow - 10.0 * u) / width);
                self.arrow_in(
                    left + arrow / 2.0,
                    baseline - size * CAP / 2.0,
                    arrow,
                    is_down,
                    if is_down { self.accent } else { self.color },
                );
                self.rate(left + arrow + 6.0 * u, baseline, size, rate, Anchor::Start);
            }
            let top = baseline + 14.0 * u;
            let chart = Chart {
                x: PAD,
                y: top,
                w: iw,
                h: PAD + ih - top,
            };
            self.chart_frame(chart);
            self.series(chart, &graph.down, self.accent, true);
            self.series(chart, &graph.up, self.color, false);
            self.text(
                chart.x + chart.w - 4.0 * u,
                chart.y + peak_font.size * CAP + 4.0 * u,
                Anchor::End,
                peak_font,
                &peak,
            );
        }
    }

    pub(super) fn chart_frame(&mut self, chart: Chart) {
        if chart.w <= 0.0 || chart.h <= 0.0 {
            return;
        }
        let hex = self.color.hex();
        let _ = write!(
            self.svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{hex}" fill-opacity="0.09"/>"#,
            chart.x,
            chart.y,
            chart.w,
            chart.h,
            (4.0 * self.unit).min(chart.h / 2.0),
        );
        let middle = chart.y + chart.h / 2.0;
        let dash = 4.0 * self.unit;
        let _ = write!(
            self.svg,
            r#"<line x1="{}" y1="{middle}" x2="{}" y2="{middle}" stroke="{hex}" stroke-opacity="0.18" stroke-width="1" stroke-dasharray="{dash} {dash}"/>"#,
            chart.x,
            chart.x + chart.w,
        );
    }

    /// Levels from 0 to 1, oldest first, spread over the chart with the last at its right edge.
    /// A missing reading breaks the line. `area` fills below it.
    pub(super) fn series(
        &mut self,
        chart: Chart,
        points: &[Option<f32>],
        color: Color,
        area: bool,
    ) {
        if chart.w <= 0.0 || chart.h <= 0.0 || points.len() < 2 {
            return;
        }
        let step = chart.w / (points.len() - 1) as f32;
        let bottom = chart.y + chart.h;
        let stroke = (2.0 * self.unit).max(1.5);
        let room = chart.h - stroke;
        let mut runs: Vec<Vec<(f32, f32)>> = Vec::new();
        let mut run = Vec::new();
        for (i, point) in points.iter().enumerate() {
            match point {
                Some(level) => run.push((
                    chart.x + step * i as f32,
                    bottom - stroke / 2.0 - level.clamp(0.0, 1.0) * room,
                )),
                None if !run.is_empty() => runs.push(std::mem::take(&mut run)),
                None => {}
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
        let hex = color.hex();
        for run in runs.iter().filter(|run| run.len() > 1) {
            let line: String = run
                .iter()
                .enumerate()
                .map(|(i, (x, y))| format!("{}{x:.1} {y:.1}", if i == 0 { 'M' } else { 'L' }))
                .collect();
            if area {
                let (first, last) = (run[0].0, run[run.len() - 1].0);
                let _ = write!(
                    self.svg,
                    r#"<path d="{line}L{last:.1} {bottom:.1}L{first:.1} {bottom:.1}Z" fill="{hex}" fill-opacity="0.28"/>"#,
                );
            }
            let _ = write!(
                self.svg,
                r#"<path d="{line}" fill="none" stroke="{hex}" stroke-opacity="{}" stroke-width="{stroke}" stroke-linejoin="round" stroke-linecap="round"/>"#,
                if area { 1.0 } else { 0.75 },
            );
        }
    }

    /// An arrow `size` tall pointing down, or up, centered at `cx`, `cy`.
    fn arrow(&mut self, cx: f32, cy: f32, size: f32, down: bool) {
        self.arrow_in(cx, cy, size, down, self.accent);
    }

    fn arrow_in(&mut self, cx: f32, cy: f32, size: f32, down: bool, color: Color) {
        let half = size / 2.0;
        let (tip, tail) = if down {
            (cy + half, cy - half)
        } else {
            (cy - half, cy + half)
        };
        let wing = size * 0.38;
        let back = if down { -wing } else { wing };
        let _ = write!(
            self.svg,
            r#"<path d="M{cx} {tail}L{cx} {tip}M{} {}L{cx} {tip}L{} {}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
            cx - wing,
            tip + back,
            cx + wing,
            tip + back,
            color.hex(),
            (size * 0.16).max(2.0),
        );
    }

    fn network(&mut self, down: &Rate, up: &Rate) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let rates = [
            (down, true, tr!("widgets.network.down")),
            (up, false, tr!("widgets.network.up")),
        ];
        if self.wide() {
            let column = iw / 2.0;
            let label_font = Font::new(16.0 * u, 600, self.color).dim(0.6).spaced(0.14);
            let unit_scale = 0.42;
            let value_gap = 0.12;
            let width = rates
                .iter()
                .map(|(rate, _, _)| {
                    tabular_width(&rate.value, 1.0)
                        + value_gap
                        + text_width(&rate.unit, Font::new(unit_scale, 600, self.color))
                })
                .fold(0.0, f32::max);
            let label_gap = 18.0 * u;
            let label_height = label_font.size * CAP;
            let size = (62.0 * u)
                .min((column - 20.0 * u) / width)
                .min((ih - label_height - label_gap) / CAP);
            let total = label_height + label_gap + size * CAP;
            let top = PAD + (ih - total) / 2.0;
            for (i, (rate, is_down, label)) in rates.into_iter().enumerate() {
                let center = PAD + column * (i as f32 + 0.5);
                let arrow = label_font.size * 1.1;
                let label_width = text_width(label, label_font);
                let header = arrow + 8.0 * u + label_width;
                let header_left = center - header / 2.0;
                self.arrow(
                    header_left + arrow / 2.0,
                    top + label_height / 2.0,
                    arrow,
                    is_down,
                );
                self.text(
                    header_left + arrow + 8.0 * u,
                    top + label_height,
                    Anchor::Start,
                    label_font,
                    label,
                );
                self.rate(center, top + total, size, rate, Anchor::Middle);
            }
            if iw > 0.0 {
                let _ = write!(
                    self.svg,
                    r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" stroke="{}" stroke-opacity="0.14" stroke-width="2"/>"#,
                    top,
                    top + total,
                    self.color.hex(),
                    x = PAD + column,
                );
            }
        } else {
            let row = ih / 2.0;
            let arrow = 26.0 * u;
            let room = iw - arrow - 12.0 * u;
            for (i, (rate, is_down, _)) in rates.into_iter().enumerate() {
                let width = tabular_width(&rate.value, 1.0)
                    + 0.12
                    + text_width(&rate.unit, Font::new(0.42, 600, self.color));
                let size = (46.0 * u).min(room / width).min(row * 0.7 / CAP);
                let baseline = PAD + row * i as f32 + (row + size * CAP) / 2.0;
                self.arrow(
                    PAD + arrow / 2.0,
                    baseline - size * CAP / 2.0,
                    arrow,
                    is_down,
                );
                self.rate(PAD + arrow + 12.0 * u, baseline, size, rate, Anchor::Start);
            }
        }
    }

    fn rate(&mut self, x: f32, baseline: f32, size: f32, rate: &Rate, anchor: Anchor) {
        let unit_font = Font::new(size * 0.42, 600, self.color).dim(0.65);
        let value_width = tabular_width(&rate.value, size);
        let gap = size * 0.12;
        let width = value_width + gap + text_width(&rate.unit, unit_font);
        let left = anchor.start(x, width);
        self.tabular(
            left,
            baseline,
            Anchor::Start,
            Font::new(size, 700, self.color),
            &rate.value,
        );
        self.text(
            left + value_width + gap,
            baseline,
            Anchor::Start,
            unit_font,
            &rate.unit,
        );
    }

    /// A rounded bar of `level`, over a faint track.
    pub(super) fn bar(&mut self, x: f32, y: f32, width: f32, height: f32, level: f32, fill: Color) {
        let radius = height / 2.0;
        let _ = write!(
            self.svg,
            r#"<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="{radius}" fill="{}" fill-opacity="0.16"/>"#,
            self.color.hex(),
        );
        let filled = width * level.clamp(0.0, 1.0);
        if filled > 0.0 {
            let _ = write!(
                self.svg,
                r#"<rect x="{x}" y="{y}" width="{}" height="{height}" rx="{radius}" fill="{}"/>"#,
                filled.max(height),
                fill.hex(),
            );
        }
    }

    fn world_clocks(&mut self, clocks: &[ClockFace]) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        if clocks.is_empty() {
            let font = Font::new(24.0 * u, 600, self.color).dim(0.6);
            let hint = tr!("widgets.worldClocks.addCity");
            let font = font.fit(hint, iw);
            self.text(
                self.w / 2.0,
                self.h / 2.0 + font.size * CAP / 2.0,
                Anchor::Middle,
                font,
                hint,
            );
            return;
        }
        if !self.wide() {
            let shown = &clocks[..clocks.len().min(2)];
            let row = ih / shown.len() as f32;
            for (i, clock) in shown.iter().enumerate() {
                let top = PAD + row * i as f32;
                let label = clock.label.to_uppercase();
                let label_font = self.label_font().sized(15.0 * u).fit(&label, iw);
                let time_font = Font::new(1.0, 700, self.color);
                let size = (row - label_font.size * CAP - 10.0 * u) / CAP;
                let size = size
                    .min(iw / tabular_width(&clock.time, 1.0).max(0.01))
                    .min(52.0 * u);
                let block = label_font.size * CAP + 8.0 * u + size * CAP;
                let y = top + (row - block) / 2.0 + label_font.size * CAP;
                self.text(self.w / 2.0, y, Anchor::Middle, label_font, &label);
                let font = if clock.night {
                    time_font.sized(size).dim(0.72)
                } else {
                    time_font.sized(size)
                };
                self.tabular(
                    self.w / 2.0,
                    y + 8.0 * u + size * CAP,
                    Anchor::Middle,
                    font,
                    &clock.time,
                );
            }
            return;
        }
        let count = clocks.len() as f32;
        let column = iw / count;
        let room = column - 16.0 * u;
        let notes: Vec<String> = clocks
            .iter()
            .map(|clock| {
                [clock.suffix.as_deref(), clock.day.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .collect();
        // Every column the same sizes and heights, so the times line up.
        let label_font = clocks
            .iter()
            .map(|c| {
                self.label_font()
                    .fit(&c.label.to_uppercase(), room - 18.0 * u)
            })
            .fold(
                self.label_font(),
                |a, b| if b.size < a.size { b } else { a },
            );
        let note_font = notes
            .iter()
            .map(|n| Font::new(18.0 * u, 600, self.color).dim(0.65).fit(n, room))
            .fold(Font::new(18.0 * u, 600, self.color).dim(0.65), |a, b| {
                if b.size < a.size { b } else { a }
            });
        let size = clocks
            .iter()
            .map(|c| room / tabular_width(&c.time, 1.0).max(0.01))
            .fold(64.0 * u / count.sqrt(), f32::min);
        let any_note = notes.iter().any(|n| !n.is_empty());
        let block = label_font.size * CAP
            + 18.0 * u
            + size * CAP
            + if any_note {
                16.0 * u + note_font.size * CAP
            } else {
                0.0
            };
        for (i, clock) in clocks.iter().enumerate() {
            let left = PAD + column * i as f32;
            let center = left + column / 2.0;
            if i > 0 {
                let _ = write!(
                    self.svg,
                    r#"<line x1="{left}" y1="{}" x2="{left}" y2="{}" stroke="{}" stroke-opacity="0.18" stroke-width="{}"/>"#,
                    PAD + 10.0 * u,
                    self.h - PAD - 10.0 * u,
                    self.color.hex(),
                    1.5 * u,
                );
            }
            let label = clock.label.to_uppercase();
            let note = &notes[i];
            let time_font = Font::new(size, 700, self.color);
            let mut y = PAD + (ih - block) / 2.0 + label_font.size * CAP;
            let dot = 5.0 * u;
            let label_width = text_width(&label, label_font);
            let dot_x = center - label_width / 2.0 - dot - 7.0 * u;
            let dot_y = y - label_font.size * CAP / 2.0;
            if clock.night {
                let _ = write!(
                    self.svg,
                    r#"<circle cx="{dot_x}" cy="{dot_y}" r="{dot}" fill="none" stroke="{}" stroke-width="{}"/>"#,
                    self.accent.hex(),
                    1.8 * u,
                );
            } else {
                let _ = write!(
                    self.svg,
                    r#"<circle cx="{dot_x}" cy="{dot_y}" r="{dot}" fill="{}"/>"#,
                    self.accent.hex(),
                );
            }
            self.text(
                center + (dot + 7.0 * u) / 2.0,
                y,
                Anchor::Middle,
                label_font,
                &label,
            );
            y += 18.0 * u + size * CAP;
            self.tabular(
                center,
                y,
                Anchor::Middle,
                time_font.sized(size),
                &clock.time,
            );
            if !note.is_empty() {
                y += 16.0 * u + note_font.size * CAP;
                self.text(center, y, Anchor::Middle, note_font, note);
            }
        }
    }

    fn calendar(&mut self, weekday: &str, day: &str, month: &str, weeks: &[[u8; 7]], today: u8) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let weekday = weekday.to_uppercase();
        if !self.wide() {
            let label_font = self.label_font().fit(&weekday, iw);
            let month_font = Font::new(20.0 * u, 600, self.color).dim(0.7).fit(month, iw);
            let size = (ih - label_font.size * CAP - month_font.size * CAP - 28.0 * u) / CAP;
            let size = size.min(96.0 * u);
            let total =
                label_font.size * CAP + 14.0 * u + size * CAP + 14.0 * u + month_font.size * CAP;
            let mut y = PAD + (ih - total) / 2.0 + label_font.size * CAP;
            self.text(self.w / 2.0, y, Anchor::Middle, label_font, &weekday);
            y += 14.0 * u + size * CAP;
            self.tabular(
                self.w / 2.0,
                y,
                Anchor::Middle,
                Font::new(size, 700, self.color),
                day,
            );
            y += 14.0 * u + month_font.size * CAP;
            self.text(self.w / 2.0, y, Anchor::Middle, month_font, month);
            return;
        }
        let side = iw * 0.36;
        let center = PAD + side / 2.0;
        let label_font = self.label_font().fit(&weekday, side);
        let month_font = Font::new(18.0 * u, 600, self.color)
            .dim(0.7)
            .fit(month, side);
        let size = (ih - label_font.size * CAP - month_font.size * CAP - 28.0 * u) / CAP;
        let size = size
            .min(84.0 * u)
            .min(side / tabular_width(day, 1.0).max(0.01));
        let total =
            label_font.size * CAP + 14.0 * u + size * CAP + 14.0 * u + month_font.size * CAP;
        let mut y = PAD + (ih - total) / 2.0 + label_font.size * CAP;
        self.text(center, y, Anchor::Middle, label_font, &weekday);
        y += 14.0 * u + size * CAP;
        self.tabular(
            center,
            y,
            Anchor::Middle,
            Font::new(size, 700, self.color),
            day,
        );
        y += 14.0 * u + month_font.size * CAP;
        self.text(center, y, Anchor::Middle, month_font, month);

        let grid_left = PAD + side + 14.0 * u;
        let grid_width = self.w - PAD - grid_left;
        let rows = weeks.len() as f32 + 1.0;
        let row = ih / rows;
        let column = grid_width / 7.0;
        let font = Font::new((row * 0.62).min(column * 0.62), 600, self.color);
        let baseline = |r: f32| PAD + row * r + (row + font.size * CAP) / 2.0;
        for (c, letter) in week_letters().iter().enumerate() {
            let x = grid_left + column * (c as f32 + 0.5);
            self.text(
                x,
                baseline(0.0),
                Anchor::Middle,
                Font {
                    fill: self.accent,
                    ..font
                }
                .dim(0.9),
                letter,
            );
        }
        for (r, week) in weeks.iter().enumerate() {
            for (c, &date) in week.iter().enumerate() {
                if date == 0 {
                    continue;
                }
                let x = grid_left + column * (c as f32 + 0.5);
                let label = date.to_string();
                if date == today {
                    let radius = (row.min(column) * 0.5).max(1.0);
                    let _ = write!(
                        self.svg,
                        r#"<circle cx="{x}" cy="{}" r="{radius}" fill="{}"/>"#,
                        PAD + row * (r as f32 + 1.5),
                        self.accent.hex(),
                    );
                    self.text(
                        x,
                        baseline(r as f32 + 1.0),
                        Anchor::Middle,
                        Font {
                            weight: 800,
                            ..font
                        },
                        &label,
                    );
                } else {
                    let dim = if c >= 5 { 0.5 } else { 0.85 };
                    self.text(
                        x,
                        baseline(r as f32 + 1.0),
                        Anchor::Middle,
                        font.dim(dim),
                        &label,
                    );
                }
            }
        }
    }

    fn timer(&mut self, label: Option<&str>, main: &str, progress: f32, phase: TimerPhase) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let label = label.map(|text| {
            let text = text.to_uppercase();
            let font = self.label_font().fit(&text, iw);
            (text, font)
        });
        let hint = match phase {
            TimerPhase::Idle => Some(tr!("widgets.timer.pressToStart")),
            TimerPhase::Paused => Some(tr!("widgets.timer.paused")),
            TimerPhase::Running | TimerPhase::Done => None,
        };
        let hint_font = Font::new(16.0 * u, 600, self.color).dim(0.6);
        let hint = hint.map(|text| (text, hint_font.fit(text, iw)));
        let bar = (9.0 * u).max(4.0);
        let clock = main.chars().all(|c| c.is_ascii_digit() || c == ':');
        let main_font = Font::new(
            1.0,
            700,
            if phase == TimerPhase::Done {
                self.accent
            } else {
                self.color
            },
        );
        let width = if clock {
            tabular_width(main, 1.0)
        } else {
            text_width(main, main_font)
        };
        let above = label.as_ref().map_or(0.0, |(_, f)| f.size * CAP + 12.0 * u);
        let below = 16.0 * u + bar + hint.as_ref().map_or(0.0, |(_, f)| 12.0 * u + f.size * CAP);
        let size = (78.0 * u)
            .min(iw / width.max(0.01))
            .min((ih - above - below) / CAP);
        let total = above + size * CAP + below;
        let center = self.w / 2.0;
        let mut y = PAD + (ih - total) / 2.0;
        if let Some((text, font)) = &label {
            y += font.size * CAP;
            self.text(center, y, Anchor::Middle, *font, text);
            y += 12.0 * u;
        }
        y += size * CAP;
        let font = if phase == TimerPhase::Paused {
            main_font.sized(size).dim(0.55)
        } else {
            main_font.sized(size)
        };
        if clock {
            self.tabular(center, y, Anchor::Middle, font, main);
        } else {
            self.text(center, y, Anchor::Middle, font, main);
        }
        y += 16.0 * u;
        let accent = self.accent;
        self.bar(PAD, y, iw, bar, progress, accent);
        y += bar;
        if let Some((text, font)) = hint {
            y += 12.0 * u + font.size * CAP;
            self.text(center, y, Anchor::Middle, font, text);
        }
    }

    /// A battery outline with its charge, and a bolt while it charges.
    fn battery_icon(
        &mut self,
        Chart {
            x,
            y,
            w: width,
            h: height,
        }: Chart,
        level: Option<f32>,
        fill: Color,
        charging: bool,
    ) {
        let u = self.unit;
        let stroke = (4.0 * u).max(1.5);
        let cap = width * 0.06;
        let body = width - cap;
        let radius = height * 0.2;
        let _ = write!(
            self.svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{radius}" fill="none" stroke="{}" stroke-opacity="0.75" stroke-width="{stroke}"/><rect x="{}" y="{}" width="{cap}" height="{}" rx="{}" fill="{}" fill-opacity="0.75"/>"#,
            x + stroke / 2.0,
            y + stroke / 2.0,
            body - stroke,
            height - stroke,
            self.color.hex(),
            x + body + stroke * 0.25,
            y + height * 0.3,
            height * 0.4,
            cap / 3.0,
            self.color.hex(),
        );
        if let Some(level) = level {
            let gap = stroke * 1.8;
            let inner = body - 2.0 * gap;
            let filled = inner * level.clamp(0.0, 1.0);
            if filled > 0.0 {
                let _ = write!(
                    self.svg,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}"/>"#,
                    x + gap,
                    y + gap,
                    filled.max(radius * 0.6),
                    height - 2.0 * gap,
                    (radius - gap / 2.0).max(1.0),
                    fill.hex(),
                );
            }
        }
        if charging {
            let (cx, cy, s) = (x + body / 2.0, y + height / 2.0, height * 0.42);
            let points = [
                (0.15, -1.0),
                (-0.55, 0.12),
                (-0.02, 0.12),
                (-0.15, 1.0),
                (0.55, -0.12),
                (0.02, -0.12),
            ]
            .iter()
            .map(|(px, py)| format!("{},{}", cx + px * s, cy + py * s))
            .collect::<Vec<_>>()
            .join(" ");
            let _ = write!(
                self.svg,
                r##"<polygon points="{points}" fill="#ffffff" stroke="#000000" stroke-opacity="0.55" stroke-width="{}" stroke-linejoin="round"/>"##,
                2.0 * u,
            );
        }
    }

    fn battery(&mut self, level: Option<f32>, text: &str, detail: &str, charging: bool, low: bool) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let fill = if low { RED } else { self.accent };
        let value_font = Font::new(1.0, 700, if low { RED } else { self.color });
        let detail_font = Font::new(19.0 * u, 600, self.color).dim(0.7);
        if self.wide() {
            let icon_w = iw * 0.38;
            let icon_h = (icon_w * 0.5).min(ih * 0.62);
            let icon_y = PAD + (ih - icon_h) / 2.0;
            self.battery_icon(
                Chart {
                    x: PAD,
                    y: icon_y,
                    w: icon_w,
                    h: icon_h,
                },
                level,
                fill,
                charging,
            );
            let left = PAD + icon_w + 22.0 * u;
            let room = self.w - PAD - left;
            let size = (66.0 * u).min(room / tabular_width(text, 1.0).max(0.01));
            let detail_font = detail_font.fit(detail, room);
            let block = size * CAP
                + if detail.is_empty() {
                    0.0
                } else {
                    16.0 * u + detail_font.size * CAP
                };
            let mut y = PAD + (ih - block) / 2.0 + size * CAP;
            self.tabular(left, y, Anchor::Start, value_font.sized(size), text);
            if !detail.is_empty() {
                y += 16.0 * u + detail_font.size * CAP;
                self.text(left, y, Anchor::Start, detail_font, detail);
            }
            return;
        }
        let icon_w = iw * 0.62;
        let icon_h = icon_w * 0.46;
        let detail_font = detail_font.sized(15.0 * u).fit(detail, iw);
        let size = (48.0 * u).min(iw / tabular_width(text, 1.0).max(0.01));
        let block = icon_h
            + 14.0 * u
            + size * CAP
            + if detail.is_empty() {
                0.0
            } else {
                12.0 * u + detail_font.size * CAP
            };
        let mut y = PAD + (ih - block) / 2.0;
        self.battery_icon(
            Chart {
                x: self.w / 2.0 - icon_w / 2.0,
                y,
                w: icon_w,
                h: icon_h,
            },
            level,
            fill,
            charging,
        );
        y += icon_h + 14.0 * u + size * CAP;
        self.tabular(
            self.w / 2.0,
            y,
            Anchor::Middle,
            value_font.sized(size),
            text,
        );
        if !detail.is_empty() {
            y += 12.0 * u + detail_font.size * CAP;
            self.text(self.w / 2.0, y, Anchor::Middle, detail_font, detail);
        }
    }

    /// Two bars while it plays, a triangle while paused.
    fn play_state(&mut self, cx: f32, cy: f32, r: f32, playing: bool) {
        let _ = write!(
            self.svg,
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{}"/>"#,
            self.accent.hex()
        );
        let s = r * 0.42;
        if playing {
            let w = s * 0.55;
            for x in [cx - s * 0.55 - w / 2.0, cx + s * 0.55 - w / 2.0] {
                let _ = write!(
                    self.svg,
                    r##"<rect x="{x}" y="{}" width="{w}" height="{}" rx="{}" fill="#ffffff"/>"##,
                    cy - s,
                    2.0 * s,
                    w * 0.25,
                );
            }
        } else {
            let _ = write!(
                self.svg,
                r##"<polygon points="{},{} {},{} {},{}" fill="#ffffff"/>"##,
                cx - s * 0.6,
                cy - s,
                cx - s * 0.6,
                cy + s,
                cx + s,
                cy,
            );
        }
    }

    fn now_playing(
        &mut self,
        title: Option<&str>,
        artist: &str,
        playing: bool,
        progress: Option<f32>,
        time: Option<&str>,
    ) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let Some(title) = title else {
            let font = Font::new(24.0 * u, 600, self.color).dim(0.6);
            let font = font.fit(artist, iw);
            self.text(
                self.w / 2.0,
                self.h / 2.0 + font.size * CAP / 2.0,
                Anchor::Middle,
                font,
                artist,
            );
            return;
        };
        let wide = self.wide();
        let bar = (8.0 * u).max(3.0);
        let time_font = Font::new(if wide { 16.0 } else { 13.0 } * u, 600, self.color).dim(0.65);
        let footer = progress.map_or(0.0, |_| bar + 12.0 * u)
            + time.map_or(0.0, |_| time_font.size * CAP + 10.0 * u);
        let (left, room, top_room) = if wide {
            let r = (ih - footer).min(76.0 * u) / 2.0 * 0.9;
            self.play_state(PAD + r, PAD + (ih - footer) / 2.0, r, playing);
            (
                PAD + 2.0 * r + 18.0 * u,
                iw - 2.0 * r - 18.0 * u,
                ih - footer,
            )
        } else {
            (PAD, iw, ih - footer)
        };
        let title_font = Font::new(if wide { 30.0 } else { 22.0 } * u, 700, self.color);
        let title_text = ellipsize(title, title_font, room);
        let artist_font = Font::new(if wide { 20.0 } else { 16.0 } * u, 600, self.color).dim(0.7);
        let artist_text = ellipsize(artist, artist_font, room);
        let block = title_font.size * CAP
            + if artist.is_empty() {
                0.0
            } else {
                12.0 * u + artist_font.size * CAP
            };
        let mut y = PAD + (top_room - block) / 2.0 + title_font.size * CAP;
        let anchor_x = if wide { left } else { self.w / 2.0 };
        let anchor = if wide { Anchor::Start } else { Anchor::Middle };
        self.text(anchor_x, y, anchor, title_font, &title_text);
        if !artist.is_empty() {
            y += 12.0 * u + artist_font.size * CAP;
            self.text(anchor_x, y, anchor, artist_font, &artist_text);
        }
        let mut y = PAD + top_room;
        if let Some(progress) = progress {
            y += 12.0 * u;
            let accent = self.accent;
            self.bar(PAD, y - bar / 2.0, iw, bar, progress, accent);
            y += bar / 2.0;
        }
        if let Some(time) = time {
            y += 10.0 * u + time_font.size * CAP;
            self.tabular(self.w - PAD, y, Anchor::End, time_font, time);
        }
    }

    fn latency(
        &mut self,
        value: &str,
        target: &str,
        history: &[Option<f32>],
        peak: &str,
        ok: bool,
    ) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let value_font = Font::new(1.0, 700, if ok { self.color } else { RED });
        let target_font = Font::new(17.0 * u, 600, self.color).dim(0.65);
        let peak_font = Font::new(13.0 * u, 600, self.color).dim(0.55);
        if self.wide() {
            let side = iw * 0.4;
            let size = (52.0 * u).min(side / text_width(value, value_font).max(0.01));
            let target_font = target_font.fit(target, side);
            let block = size * CAP + 14.0 * u + target_font.size * CAP;
            let y = PAD + (ih - block) / 2.0 + size * CAP;
            self.text(PAD, y, Anchor::Start, value_font.sized(size), value);
            self.text(
                PAD,
                y + 14.0 * u + target_font.size * CAP,
                Anchor::Start,
                target_font,
                target,
            );
            let chart = Chart {
                x: PAD + side + 16.0 * u,
                y: PAD + peak_font.size * CAP + 8.0 * u,
                w: iw - side - 16.0 * u,
                h: ih - peak_font.size * CAP - 8.0 * u,
            };
            self.chart_frame(chart);
            let accent = self.accent;
            self.series(chart, history, accent, true);
            if !peak.is_empty() {
                self.text(
                    chart.x + chart.w,
                    PAD + peak_font.size * CAP,
                    Anchor::End,
                    peak_font,
                    peak,
                );
            }
            return;
        }
        let size = (40.0 * u).min(iw / text_width(value, value_font).max(0.01));
        let target_font = target_font.sized(14.0 * u).fit(target, iw);
        let mut y = PAD + size * CAP;
        self.text(
            self.w / 2.0,
            y,
            Anchor::Middle,
            value_font.sized(size),
            value,
        );
        y += 10.0 * u + target_font.size * CAP;
        self.text(self.w / 2.0, y, Anchor::Middle, target_font, target);
        let chart = Chart {
            x: PAD,
            y: y + 12.0 * u,
            w: iw,
            h: (self.h - PAD - y - 12.0 * u).max(0.0),
        };
        self.chart_frame(chart);
        let accent = self.accent;
        self.series(chart, history, accent, true);
    }

    /// The disc, its lit part bounded by the edge on one side and the terminator on the other.
    fn moon_disc(&mut self, cx: f32, cy: f32, r: f32, phase: f32) {
        let _ = write!(
            self.svg,
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{}" fill-opacity="0.13"/>"#,
            self.color.hex(),
        );
        let lit = (1.0 - (2.0 * std::f32::consts::PI * phase).cos()) / 2.0;
        if lit < 0.01 {
            return;
        }
        let rx = r * (1.0 - 2.0 * lit).abs();
        let waxing = phase < 0.5;
        let crescent = lit < 0.5;
        let (edge, terminator) = match (waxing, crescent) {
            (true, true) => (1, 0),
            (true, false) => (1, 1),
            (false, true) => (0, 1),
            (false, false) => (0, 0),
        };
        let (top, bottom) = (cy - r, cy + r);
        let _ = write!(
            self.svg,
            r#"<path d="M{cx} {top} A{r} {r} 0 0 {edge} {cx} {bottom} A{rx} {r} 0 0 {terminator} {cx} {top}Z" fill="{}"/>"#,
            self.color.hex(),
        );
    }

    fn moon(&mut self, phase: f32, name: &str, detail: &str) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let name_font = Font::new(28.0 * u, 700, self.color);
        let detail_font = Font::new(18.0 * u, 600, self.color).dim(0.65);
        if self.wide() {
            let r = ih * 0.44;
            self.moon_disc(PAD + r, PAD + ih / 2.0, r, phase);
            let left = PAD + 2.0 * r + 24.0 * u;
            let room = self.w - PAD - left;
            let name_font = name_font.fit(name, room);
            let detail_font = detail_font.fit(detail, room);
            let block = name_font.size * CAP + 14.0 * u + detail_font.size * CAP;
            let y = PAD + (ih - block) / 2.0 + name_font.size * CAP;
            self.text(left, y, Anchor::Start, name_font, name);
            self.text(
                left,
                y + 14.0 * u + detail_font.size * CAP,
                Anchor::Start,
                detail_font,
                detail,
            );
            return;
        }
        let name_font = name_font.sized(20.0 * u).fit(name, iw);
        let detail_font = detail_font.sized(14.0 * u).fit(detail, iw);
        let text = name_font.size * CAP + 10.0 * u + detail_font.size * CAP;
        let r = ((ih - text - 14.0 * u) / 2.0).min(iw / 2.0);
        self.moon_disc(self.w / 2.0, PAD + r, r, phase);
        let mut y = PAD + 2.0 * r + 14.0 * u + name_font.size * CAP;
        self.text(self.w / 2.0, y, Anchor::Middle, name_font, name);
        y += 10.0 * u + detail_font.size * CAP;
        self.text(self.w / 2.0, y, Anchor::Middle, detail_font, detail);
    }

    /// The hour big in the middle; an empty lead or rest takes no room.
    fn word_clock(&mut self, lead: &str, hour: &str, rest: &str) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let lead_font = Font::new(24.0 * u, 600, self.color).dim(0.7).fit(lead, iw);
        let rest_font = Font::new(34.0 * u, 700, self.color).fit(rest, iw);
        let gap = 12.0 * u;
        let height = |text: &str, font: Font| {
            if text.is_empty() {
                0.0
            } else {
                font.size * CAP + gap
            }
        };
        let (lead_height, rest_height) = (height(lead, lead_font), height(rest, rest_font));
        let room = ih - lead_height - rest_height;
        let hour_font = Font::new((room / CAP).min(70.0 * u), 800, self.accent).fit(hour, iw);
        let total = lead_height + hour_font.size * CAP + rest_height;
        let center = self.w / 2.0;
        let mut y = PAD + (ih - total) / 2.0;
        if !lead.is_empty() {
            y += lead_font.size * CAP;
            self.text(center, y, Anchor::Middle, lead_font, lead);
            y += gap;
        }
        y += hour_font.size * CAP;
        self.text(center, y, Anchor::Middle, hour_font, hour);
        if !rest.is_empty() {
            y += gap + rest_font.size * CAP;
            self.text(center, y, Anchor::Middle, rest_font, rest);
        }
    }

    /// As big as it fits, in three lines at most on the wide key and five on a square one.
    fn phrase(&mut self, text: &str) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let max_lines = if self.wide() { 3 } else { 5 };
        let line_gap = 0.32;
        let mut size = 56.0 * u;
        let (font, lines) = loop {
            let font = Font::new(size, 700, self.color);
            let lines = wrap(text, font, iw);
            let height =
                size * CAP * lines.len() as f32 + size * line_gap * (lines.len() as f32 - 1.0);
            let fits = lines.len() <= max_lines
                && height <= ih
                && lines.iter().all(|l| text_width(l, font) <= iw);
            if fits || size <= 12.0 * u {
                break (font, lines);
            }
            size -= 2.0 * u;
        };
        let count = lines.len() as f32;
        let height = size * CAP * count + size * line_gap * (count - 1.0);
        let mut y = PAD + (ih - height) / 2.0 + size * CAP;
        for line in &lines {
            let line = ellipsize(line, font, iw);
            self.text(self.w / 2.0, y, Anchor::Middle, font, &line);
            y += size * CAP + size * line_gap;
        }
    }

    fn countdown(&mut self, label: Option<&str>, main: &str, detail: Option<&str>) {
        let (iw, ih) = self.inner();
        let u = self.unit;
        let label = label.map(|text| {
            let text = text.to_uppercase();
            let font = self.label_font().fit(&text, iw);
            (text, font)
        });
        let clock = main.chars().all(|c| c.is_ascii_digit() || c == ':');
        let detail_font = Font::new(28.0 * u, 600, self.color).dim(0.72);
        let detail = detail.map(|text| {
            let size = detail_font
                .size
                .min(iw / tabular_width(text, 1.0).max(0.01));
            (text, detail_font.sized(size))
        });
        let label_gap = 14.0 * u;
        let detail_gap = 16.0 * u;
        let above = label
            .as_ref()
            .map_or(0.0, |(_, f)| f.size * CAP + label_gap);
        let below = detail
            .as_ref()
            .map_or(0.0, |(_, f)| detail_gap + f.size * CAP);
        let main_font = Font::new(1.0, 700, self.color);
        let width = if clock {
            tabular_width(main, 1.0)
        } else {
            text_width(main, main_font)
        };
        let size = (if clock { 78.0 } else { 76.0 } * u)
            .min(iw / width.max(0.01))
            .min((ih - above - below) / CAP);
        let total = above + size * CAP + below;
        let center = self.w / 2.0;
        let mut y = PAD + (ih - total) / 2.0;
        if let Some((text, font)) = &label {
            y += font.size * CAP;
            self.text(center, y, Anchor::Middle, *font, text);
            y += label_gap;
        }
        y += size * CAP;
        if clock {
            self.tabular(center, y, Anchor::Middle, main_font.sized(size), main);
        } else {
            self.text(center, y, Anchor::Middle, main_font.sized(size), main);
        }
        if let Some((text, font)) = detail {
            y += detail_gap + font.size * CAP;
            self.tabular(center, y, Anchor::Middle, font, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::design::{
        CoverShape, Drawing, ImageData, KeyDesign, Layer, LockState, Reading, WeatherDay,
    };
    use crate::render::{fonts, rasterize, svg};

    fn rate(value: &str, unit: &str) -> Rate {
        Rate {
            value: value.into(),
            unit: unit.into(),
        }
    }

    fn meter(label: &str, level: Option<f32>, history: Vec<Option<f32>>) -> Meter {
        Meter {
            label: label.into(),
            level,
            text: level.map_or("—".into(), |l| format!("{:.0} %", l * 100.0)),
            history,
        }
    }

    fn history() -> Vec<Option<f32>> {
        let mut points: Vec<Option<f32>> = (0..60).map(|i| Some(i as f32 / 60.0)).collect();
        points[..10].fill(None);
        points[30] = None;
        points
    }

    fn contents() -> Vec<WidgetContent> {
        let meters = vec![
            meter("CPU", Some(0.42), history()),
            meter("RAM", Some(1.0), history()),
            meter("GPU", None, vec![None; 60]),
            meter("Disco", Some(0.0), Vec::new()),
        ];
        let mut all = vec![
            WidgetContent::DigitalClock {
                time: "13:05".into(),
                seconds: Some("09".into()),
                suffix: Some("PM".into()),
                label: Some("Madrid".into()),
                line: Some("Miércoles, 4 de marzo".into()),
            },
            WidgetContent::DigitalClock {
                time: "08:05".into(),
                seconds: None,
                suffix: None,
                label: None,
                line: None,
            },
            WidgetContent::AnalogClock {
                hour: 32.5,
                minute: 30.9,
                second: Some(54.0),
                label: Some("Tokio".into()),
                weekday: Some("Miércoles".into()),
                date: Some("4 de marzo".into()),
            },
            WidgetContent::AnalogClock {
                hour: 0.0,
                minute: 0.0,
                second: None,
                label: None,
                weekday: None,
                date: None,
            },
            WidgetContent::Meters {
                meters: meters.clone(),
                graph: false,
            },
            WidgetContent::Meters {
                meters: meters.clone(),
                graph: true,
            },
            WidgetContent::Meters {
                meters: meters[..1].to_vec(),
                graph: true,
            },
            WidgetContent::Meters {
                meters: Vec::new(),
                graph: false,
            },
            WidgetContent::Network {
                down: rate("1,5", "MB/s"),
                up: rate("—", ""),
                graph: None,
            },
            WidgetContent::Network {
                down: rate("12", "KB/s"),
                up: rate("999", "B/s"),
                graph: Some(NetworkGraph {
                    down: history(),
                    up: vec![None; 60],
                    peak: rate("1,0", "KB/s"),
                }),
            },
            WidgetContent::Network {
                down: rate("0", "B/s"),
                up: rate("0", "B/s"),
                graph: Some(NetworkGraph {
                    down: Vec::new(),
                    up: vec![Some(0.5)],
                    peak: rate("1,0", "KB/s"),
                }),
            },
            WidgetContent::Countdown {
                label: Some("Año nuevo <&>".into()),
                main: "3 días".into(),
                detail: Some("01:00:00".into()),
            },
            WidgetContent::Countdown {
                label: None,
                main: "¡Ya está!".into(),
                detail: None,
            },
            WidgetContent::WorldClocks {
                clocks: vec![
                    ClockFace {
                        label: "Madrid".into(),
                        time: "13:05".into(),
                        suffix: None,
                        day: None,
                        night: false,
                    },
                    ClockFace {
                        label: "Nueva York".into(),
                        time: "7:05".into(),
                        suffix: Some("AM".into()),
                        day: None,
                        night: false,
                    },
                    ClockFace {
                        label: "Tokio".into(),
                        time: "21:05".into(),
                        suffix: None,
                        day: Some("mañana".into()),
                        night: true,
                    },
                ],
            },
            WidgetContent::WorldClocks { clocks: Vec::new() },
            WidgetContent::Calendar {
                weekday: "Miércoles".into(),
                day: "4".into(),
                month: "Marzo 2026".into(),
                weeks: vec![
                    [0, 0, 0, 0, 0, 0, 1],
                    [2, 3, 4, 5, 6, 7, 8],
                    [9, 10, 11, 12, 13, 14, 15],
                    [16, 17, 18, 19, 20, 21, 22],
                    [23, 24, 25, 26, 27, 28, 29],
                    [30, 31, 0, 0, 0, 0, 0],
                ],
                today: 4,
            },
            WidgetContent::Timer {
                label: Some("Pomodoro".into()),
                main: "18:42".into(),
                progress: 0.25,
                phase: TimerPhase::Running,
            },
            WidgetContent::Timer {
                label: None,
                main: "25:00".into(),
                progress: 0.0,
                phase: TimerPhase::Idle,
            },
            WidgetContent::Timer {
                label: None,
                main: "07:10".into(),
                progress: 0.7,
                phase: TimerPhase::Paused,
            },
            WidgetContent::Timer {
                label: Some("Té".into()),
                main: "¡Tiempo!".into(),
                progress: 1.0,
                phase: TimerPhase::Done,
            },
            WidgetContent::Battery {
                level: Some(0.62),
                text: "62 %".into(),
                detail: "Cargando · llena en 1:23".into(),
                charging: true,
                low: false,
            },
            WidgetContent::Battery {
                level: Some(0.12),
                text: "12 %".into(),
                detail: "Quedan 0:35".into(),
                charging: false,
                low: true,
            },
            WidgetContent::Battery {
                level: None,
                text: "—".into(),
                detail: "Sin batería".into(),
                charging: false,
                low: false,
            },
            WidgetContent::NowPlaying {
                title: Some("So What (feat. a very long name that does not fit)".into()),
                artist: "Miles Davis".into(),
                playing: true,
                progress: Some(0.3),
                time: Some("2:49 / 9:22".into()),
            },
            WidgetContent::NowPlaying {
                title: Some("Pausada".into()),
                artist: String::new(),
                playing: false,
                progress: None,
                time: None,
            },
            WidgetContent::NowPlaying {
                title: None,
                artist: "Nada sonando".into(),
                playing: false,
                progress: None,
                time: None,
            },
            WidgetContent::Latency {
                value: "12 ms".into(),
                target: "Router · 192.168.1.1".into(),
                history: (0..60)
                    .map(|i| (i % 17 != 0).then_some(0.2 + 0.3 * ((i as f32) / 7.0).sin().abs()))
                    .collect(),
                peak: "48 ms".into(),
                ok: true,
            },
            WidgetContent::Latency {
                value: "Sin respuesta".into(),
                target: "nas.local".into(),
                history: vec![None; 10],
                peak: String::new(),
                ok: false,
            },
            WidgetContent::Moon {
                phase: 0.2,
                name: "Creciente".into(),
                detail: "38 % · llena en 9 días".into(),
            },
            WidgetContent::Moon {
                phase: 0.62,
                name: "Gibosa menguante".into(),
                detail: "85 % · nueva en 11 días".into(),
            },
            WidgetContent::WordClock {
                lead: "Son las".into(),
                hour: "tres".into(),
                rest: "menos veinticinco".into(),
            },
            WidgetContent::WordClock {
                lead: "It's twenty-five to".into(),
                hour: "three".into(),
                rest: String::new(),
            },
            WidgetContent::Phrase {
                text: "Respira hondo y sigue adelante, que hoy va a salir todo bien".into(),
            },
            WidgetContent::Phrase {
                text: "Hola".into(),
            },
            WidgetContent::Entity {
                label: Some("Salón".into()),
                value: "21,5".into(),
                unit: Some("°C".into()),
                detail: Some("→ 22,0 °C".into()),
                extra: Some("Calefacción · Humedad 48 %".into()),
                level: None,
                on: Some(true),
                tint: None,
                icon: Some(icon()),
            },
            WidgetContent::Entity {
                label: Some("Luz del techo".into()),
                value: "Encendida".into(),
                unit: None,
                detail: Some("71 %".into()),
                extra: None,
                level: Some(0.71),
                on: Some(true),
                tint: None,
                icon: None,
            },
            WidgetContent::Entity {
                label: Some("Puerta del garaje".into()),
                value: "Cerrada".into(),
                unit: None,
                detail: None,
                extra: None,
                level: None,
                on: Some(false),
                tint: None,
                icon: Some(icon()),
            },
            WidgetContent::Entity {
                label: None,
                value: "Hoy toca sacar la basura orgánica".into(),
                unit: None,
                detail: None,
                extra: None,
                level: None,
                on: None,
                tint: None,
                icon: None,
            },
            WidgetContent::History {
                label: Some("Consumo".into()),
                value: "1,2".into(),
                unit: Some("kW".into()),
                points: history(),
                low: "0,1 kW".into(),
                high: "3,4 kW".into(),
            },
            WidgetContent::Readings {
                items: vec![
                    Reading {
                        label: "Fuera".into(),
                        value: "15,6".into(),
                        unit: Some("°C".into()),
                        on: None,
                    },
                    Reading {
                        label: "Humedad".into(),
                        value: "54".into(),
                        unit: Some("%".into()),
                        on: None,
                    },
                    Reading {
                        label: "Puerta".into(),
                        value: "Abierta".into(),
                        unit: None,
                        on: Some(true),
                    },
                ],
            },
            WidgetContent::Picture {
                image: None,
                missing: "Leyendo…".into(),
            },
            WidgetContent::Weather {
                condition: "partlycloudy".into(),
                text: "Parcialmente nublado".into(),
                temperature: "21,6°C".into(),
                detail: Some("92 % · 2 km/h".into()),
                days: [
                    ("Hoy", "rainy"),
                    ("Mar", "lightning-rainy"),
                    ("Mié", "cloudy"),
                    ("Jue", "sunny"),
                    ("Vie", "snowy"),
                ]
                .into_iter()
                .map(|(name, condition)| WeatherDay {
                    name: name.into(),
                    condition: condition.into(),
                    high: Some("22°".into()),
                    low: Some("15°".into()),
                })
                .collect(),
            },
            WidgetContent::Weather {
                condition: "clear-night".into(),
                text: "Despejado, de noche".into(),
                temperature: "12°C".into(),
                detail: None,
                days: Vec::new(),
            },
        ];
        all.extend(tiles());
        all
    }

    fn tile(
        label: &str,
        value: &str,
        unit: Option<&str>,
        on: Option<bool>,
        tint: Option<Color>,
        drawing: Drawing,
    ) -> WidgetContent {
        WidgetContent::Tile {
            label: Some(label.into()),
            value: value.into(),
            unit: unit.map(Into::into),
            detail: Some("Abierta".into()),
            extra: Some("Hace 5 min".into()),
            badge: None,
            on,
            tint,
            drawing,
        }
    }

    fn with_lines(
        mut content: WidgetContent,
        new_detail: Option<&str>,
        new_badge: Option<&str>,
    ) -> WidgetContent {
        if let WidgetContent::Tile { detail, badge, .. } = &mut content {
            *detail = new_detail.map(Into::into).or(detail.take());
            *badge = new_badge.map(Into::into);
        }
        content
    }

    fn tiles() -> Vec<WidgetContent> {
        let amber = Some(Color::rgb(255, 193, 7));
        let cover = |shape, open| Drawing::Cover {
            shape,
            open,
            tilt: Some(0.5),
        };
        let mut all = vec![
            tile(
                "Lámpara",
                "80",
                Some("%"),
                Some(true),
                amber,
                Drawing::Bulb { level: Some(0.8) },
            ),
            tile(
                "Lámpara",
                "Apagada",
                None,
                Some(false),
                None,
                Drawing::Bulb { level: None },
            ),
            with_lines(
                tile(
                    "Enchufe",
                    "Encendido",
                    None,
                    Some(true),
                    amber,
                    Drawing::Plug,
                ),
                None,
                Some("35,2 W"),
            ),
            tile("Enchufe", "Apagado", None, Some(false), None, Drawing::Plug),
            tile(
                "Interruptor",
                "Encendido",
                None,
                Some(true),
                amber,
                Drawing::Switch,
            ),
            tile(
                "Ventilador",
                "60",
                Some("%"),
                Some(true),
                Some(Color::rgb(0, 188, 212)),
                Drawing::Fan { level: Some(0.6) },
            ),
            with_lines(
                tile(
                    "Salón",
                    "21,3",
                    Some("°C"),
                    Some(true),
                    Some(Color::rgb(255, 129, 0)),
                    Drawing::Gauge {
                        level: Some(0.5),
                        marks: vec![0.55],
                        ring: false,
                    },
                ),
                Some("→ 22°"),
                Some("21 %"),
            ),
            tile(
                "Temporizador",
                "0:04:32",
                None,
                Some(true),
                amber,
                Drawing::Gauge {
                    level: Some(0.7),
                    marks: Vec::new(),
                    ring: true,
                },
            ),
            tile(
                "Puerta",
                "Bloqueada",
                None,
                Some(true),
                Some(Color::rgb(76, 175, 80)),
                Drawing::Lock {
                    state: LockState::Locked,
                },
            ),
            tile(
                "Puerta",
                "Desbloqueada",
                None,
                Some(false),
                Some(Color::rgb(244, 67, 54)),
                Drawing::Lock {
                    state: LockState::Unlocked,
                },
            ),
            tile(
                "Temperatura",
                "21,5",
                Some("°C"),
                None,
                Some(Color::rgb(255, 129, 0)),
                Drawing::Thermometer { level: 0.63 },
            ),
            tile(
                "Humedad",
                "48",
                Some("%"),
                None,
                Some(Color::rgb(43, 154, 249)),
                Drawing::Droplet { level: 0.48 },
            ),
            tile(
                "Batería",
                "15",
                Some("%"),
                None,
                Some(Color::rgb(244, 67, 54)),
                Drawing::Battery { level: 0.15 },
            ),
            tile(
                "Riego",
                "Abierta",
                None,
                Some(true),
                None,
                Drawing::Valve { open: 1.0 },
            ),
            tile(
                "Sol",
                "Sobre el horizonte",
                None,
                Some(true),
                amber,
                Drawing::Sun {
                    x: 0.3,
                    height: 0.6,
                },
            ),
            tile(
                "Robot",
                "Limpiando",
                None,
                Some(true),
                Some(Color::rgb(0, 150, 136)),
                Drawing::Vacuum,
            ),
            tile("Contador", "1234", Some("kWh"), None, None, Drawing::Value),
            tile(
                "Movimiento",
                "Detectado",
                None,
                Some(true),
                amber,
                Drawing::Icon { icon: Some(icon()) },
            ),
            tile(
                "Salón",
                "Bohemian Rhapsody",
                None,
                Some(true),
                None,
                Drawing::Picture {
                    image: None,
                    round: false,
                    icon: Some(icon()),
                },
            ),
        ];
        for shape in [
            CoverShape::Curtain,
            CoverShape::Blind,
            CoverShape::Shutter,
            CoverShape::Shade,
            CoverShape::Garage,
            CoverShape::Awning,
            CoverShape::Gate,
            CoverShape::Door,
            CoverShape::Window,
        ] {
            all.push(tile(
                "Salón",
                "40",
                Some("%"),
                Some(true),
                Some(Color::rgb(186, 104, 200)),
                cover(shape, 0.4),
            ));
        }
        all
    }

    fn icon() -> ImageData {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M12 2a7 7 0 0 0-4 12.7V17a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1v-2.3A7 7 0 0 0 12 2m-3 19a1 1 0 0 0 1 1h4a1 1 0 0 0 1-1v-1H9z"/></svg>"#;
        ImageData {
            format: crate::render::ImageFormat::Svg,
            bytes: svg.as_bytes().into(),
        }
    }

    /// Writes every widget to `target/widgets` to look at them.
    #[test]
    #[ignore = "writes images to look at"]
    fn draw_every_widget() {
        let fonts = fonts::load(false);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/widgets");
        std::fs::create_dir_all(&dir).unwrap();
        for (i, content) in contents().into_iter().enumerate() {
            let design = KeyDesign {
                layers: vec![Layer::Widget {
                    view: WidgetView {
                        color: Color::WHITE,
                        accent: Color::rgb(56, 189, 248),
                        font_family: "Inter".into(),
                        content,
                    },
                }],
                ..KeyDesign::solid(Color::rgb(24, 24, 27))
            };
            for (name, size) in [("wide", (452, 196)), ("square", (196, 196))] {
                let png = rasterize::to_png(&svg::build(&design, size), size, &fonts).unwrap();
                std::fs::write(dir.join(format!("{i:02}-{name}.png")), png).unwrap();
            }
        }
    }

    #[test]
    fn every_widget_rasterizes_on_wide_and_square_keys() {
        let fonts = fonts::load(false);
        for content in contents() {
            let design = KeyDesign {
                layers: vec![Layer::Widget {
                    view: WidgetView {
                        color: Color::WHITE,
                        accent: Color::rgb(255, 102, 0),
                        font_family: "Inter".into(),
                        content: content.clone(),
                    },
                }],
                ..KeyDesign::solid(Color::BLACK)
            };
            for size in [(452, 196), (196, 196)] {
                let svg = svg::build(&design, size);
                let png = rasterize::to_png(&svg, size, &fonts);
                assert!(png.is_ok(), "{content:?} at {size:?}: {png:?}\n{svg}");
            }
        }
    }

    fn series(points: &[Option<f32>], area: bool) -> String {
        let mut svg = String::new();
        let mut pen = Pen {
            svg: &mut svg,
            family: "Inter".into(),
            color: Color::WHITE,
            accent: Color::WHITE,
            w: 452.0,
            h: 196.0,
            unit: 1.0,
        };
        let chart = Chart {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 50.0,
        };
        pen.series(chart, points, Color::WHITE, area);
        svg
    }

    #[test]
    fn series_breaks_the_line_at_missing_readings() {
        let points = [Some(0.1), Some(0.2), None, Some(0.3), Some(0.4), Some(0.5)];
        let svg = series(&points, false);
        assert_eq!(svg.matches("<path").count(), 2, "{svg}");
        assert!(svg.contains(r#"d="M10.0 "#), "{svg}");
        assert!(svg.contains(r#"d="M70.0 "#), "{svg}");
        assert_eq!(series(&points, true).matches("<path").count(), 4);
    }

    #[test]
    fn series_skips_lone_points_and_empty_charts() {
        assert!(series(&[Some(0.5), None, Some(0.5), None], false).is_empty());
        assert!(series(&[None, None, None], false).is_empty());
        assert!(series(&[Some(0.5)], false).is_empty());
        assert_eq!(
            series(&[None, Some(0.0), Some(1.0)], false)
                .matches("<path")
                .count(),
            1
        );
    }
}
