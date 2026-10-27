use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Background {
    Solid {
        color: Color,
    },
    /// `angle` in degrees, 0 = left to right, 90 = top to bottom.
    LinearGradient {
        from: Color,
        to: Color,
        #[specta(type = specta_typescript::Number)]
        angle: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
}

impl ImageFormat {
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime.split(';').next()?.trim() {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/webp" => Some(Self::Webp),
            "image/gif" => Some(Self::Gif),
            "image/svg+xml" => Some(Self::Svg),
            _ => None,
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
            Self::Svg => "image/svg+xml",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageData {
    pub format: ImageFormat,
    #[serde(with = "base64_bytes")]
    pub bytes: Arc<[u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Whole image visible, may leave empty space.
    Contain,
    /// Fills the area, may crop.
    Cover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum VerticalPosition {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Outline {
    pub color: Color,
    #[specta(type = specta_typescript::Number)]
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layer {
    Image {
        image: ImageData,
        fit: Fit,
        /// Margin around the image as a fraction of the key size (0.0–0.5).
        inset: f32,
        /// Replaces `currentColor` in SVG images, which is how icon sets leave the color open.
        tint: Option<Color>,
    },
    /// Lines are split on `\n`.
    Text {
        content: String,
        font_family: String,
        font_weight: u16,
        size: f32,
        color: Color,
        position: VerticalPosition,
        outline: Option<Outline>,
    },
    Border {
        color: Color,
        width: f32,
        radius: f32,
    },
    /// Live content, with its values already worked out.
    Widget { view: WidgetView },
    /// A tick or a cross in a corner: how the key's last request went.
    Status { ok: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyDesign {
    pub background: Background,
    /// Painted in order, the last one on top.
    pub layers: Vec<Layer>,
}

impl KeyDesign {
    pub fn solid(color: Color) -> Self {
        Self {
            background: Background::Solid { color },
            layers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetView {
    pub color: Color,
    pub accent: Color,
    pub font_family: String,
    pub content: WidgetContent,
}

/// Only what is drawn, ready to show: texts already formatted, levels from 0 to 1 and angles in
/// degrees clockwise from 12 o'clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WidgetContent {
    DigitalClock {
        time: String,
        seconds: Option<String>,
        /// AM or PM.
        suffix: Option<String>,
        label: Option<String>,
        line: Option<String>,
    },
    AnalogClock {
        hour: f32,
        minute: f32,
        second: Option<f32>,
        label: Option<String>,
        weekday: Option<String>,
        date: Option<String>,
    },
    Meters {
        meters: Vec<Meter>,
        /// Draws each meter's history instead of a bar.
        graph: bool,
    },
    Network {
        down: Rate,
        up: Rate,
        graph: Option<NetworkGraph>,
    },
    Countdown {
        label: Option<String>,
        main: String,
        detail: Option<String>,
    },
    WorldClocks {
        clocks: Vec<ClockFace>,
    },
    Calendar {
        weekday: String,
        day: String,
        /// Month and year.
        month: String,
        /// From Monday; 0 for days of the months around.
        weeks: Vec<[u8; 7]>,
        today: u8,
    },
    Timer {
        label: Option<String>,
        /// The time left, or the text once it is done.
        main: String,
        /// How much has gone, from 0 to 1.
        progress: f32,
        phase: TimerPhase,
    },
    Battery {
        level: Option<f32>,
        text: String,
        detail: String,
        charging: bool,
        /// Running out: drawn in red.
        low: bool,
    },
    NowPlaying {
        /// None when nothing plays.
        title: Option<String>,
        artist: String,
        playing: bool,
        progress: Option<f32>,
        /// Position and length, as `1:05 / 3:42`.
        time: Option<String>,
    },
    Latency {
        /// The last answer, as `12 ms`.
        value: String,
        /// Who is asked.
        target: String,
        /// Past answers, oldest first, as fractions of `peak`; None where none came.
        history: Vec<Option<f32>>,
        /// The time at the top of the graph.
        peak: String,
        /// The last one answered.
        ok: bool,
    },
    Moon {
        /// From 0 (new) through 0.5 (full) back to 1.
        phase: f32,
        name: String,
        detail: String,
    },
    /// The time in words, in three parts so the hour can stand out.
    WordClock {
        lead: String,
        hour: String,
        rest: String,
    },
    Phrase {
        text: String,
    },
    /// A value of Home Assistant: a sensor, whether a light is on, a climate's temperature…
    Entity {
        label: Option<String>,
        value: String,
        unit: Option<String>,
        detail: Option<String>,
        /// A second line under the detail, on keys with room for it.
        extra: Option<String>,
        /// How full, from 0 to 1, for values with a range: brightness, position, battery.
        level: Option<f32>,
        /// For entities that are on or off: off is drawn dimmed.
        on: Option<bool>,
        /// The color of its state, as Home Assistant's cards show it: the light's own color,
        /// orange while heating… The style's accent when none.
        tint: Option<Color>,
        icon: Option<ImageData>,
    },
    /// A value and how it went lately.
    History {
        label: Option<String>,
        value: String,
        unit: Option<String>,
        /// Oldest first, from 0 at `low` to 1 at `high`; None where there was no value.
        points: Vec<Option<f32>>,
        low: String,
        high: String,
    },
    /// Several values side by side.
    Readings {
        items: Vec<Reading>,
    },
    Picture {
        image: Option<ImageData>,
        /// Shown while there is no picture.
        missing: String,
    },
    Weather {
        /// Home Assistant's name of the condition, such as `partlycloudy`, for its drawing.
        condition: String,
        /// The condition in words.
        text: String,
        temperature: String,
        detail: Option<String>,
        days: Vec<WeatherDay>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
    pub on: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherDay {
    pub name: String,
    pub condition: String,
    pub high: Option<String>,
    pub low: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockFace {
    pub label: String,
    pub time: String,
    /// AM or PM.
    pub suffix: Option<String>,
    /// The day there when it is not the same as here: `mañana` or `ayer`.
    pub day: Option<String>,
    pub night: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerPhase {
    Idle,
    Running,
    Paused,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    pub label: String,
    /// None when the reading is not available.
    pub level: Option<f32>,
    pub text: String,
    /// Past levels, oldest first, for the graph. None where there was no reading.
    pub history: Vec<Option<f32>>,
}

/// Past speeds, oldest first, as fractions of the fastest one shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkGraph {
    pub down: Vec<Option<f32>>,
    pub up: Vec<Option<f32>>,
    /// The speed at the top of the graph.
    pub peak: Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub value: String,
    pub unit: String,
}

mod base64_bytes {
    use std::sync::Arc;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Arc<[u8]>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<[u8]>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD
            .decode(text)
            .map(Arc::from)
            .map_err(serde::de::Error::custom)
    }
}
