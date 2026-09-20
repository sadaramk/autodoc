//! Editorial palettes. Neutral slates everywhere; exactly one accent hue.

use nunki_ir::Theme;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Accent {
    /// Electric Indigo `#4F46E5`.
    #[default]
    Indigo,
    /// Crimson Coral `#E11D48`.
    Coral,
    /// Any `#RRGGBB`.
    Custom(String),
}

impl Accent {
    pub fn parse(s: &str) -> Result<Accent, String> {
        match s.trim().to_lowercase().as_str() {
            "indigo" | "electric-indigo" => Ok(Accent::Indigo),
            "coral" | "crimson-coral" => Ok(Accent::Coral),
            hex if parse_hex(hex).is_some() => Ok(Accent::Custom(hex.to_uppercase())),
            other => Err(format!("accent `{other}` must be `indigo`, `coral` or #RRGGBB")),
        }
    }

    /// (light-mode accent, dark-mode accent). Dark mode lifts lightness so
    /// the same hue keeps contrast on the dark canvas.
    fn pair(&self) -> (String, String) {
        match self {
            Accent::Indigo => ("#4F46E5".into(), "#818CF8".into()),
            Accent::Coral => ("#E11D48".into(), "#FB7185".into()),
            Accent::Custom(hex) => {
                let rgb = parse_hex(hex).unwrap_or((79, 70, 229));
                (to_hex(rgb), to_hex(mix(rgb, (255, 255, 255), 0.3)))
            }
        }
    }
}

pub struct Palette {
    pub canvas: &'static str,
    pub card: &'static str,
    pub card_recessed: &'static str,
    pub border: &'static str,
    pub text: &'static str,
    pub muted: &'static str,
    pub edge: &'static str,
    pub edge_strong: &'static str,
    pub zone_fill: &'static str,
    pub zone_stroke: &'static str,
    pub badge: &'static str,
    pub accent: String,
    pub accent_tint: String,
    pub accent_soft: String,
}

pub fn palette(theme: Theme, accent: &Accent) -> Palette {
    let (light_accent, dark_accent) = accent.pair();
    match theme {
        Theme::EditorialLight => {
            let a = parse_hex(&light_accent).unwrap();
            Palette {
                canvas: "#F8F9FA",
                card: "#FFFFFF",
                card_recessed: "#F4F6F8",
                border: "#E2E8F0",
                text: "#0F172A",
                muted: "#64748B",
                edge: "#A3B0C2",
                edge_strong: "#334155",
                zone_fill: "#F1F4F7",
                zone_stroke: "#D5DDE7",
                badge: "#F8FAFC",
                accent_tint: to_hex(mix((255, 255, 255), a, 0.06)),
                accent_soft: to_hex(mix((255, 255, 255), a, 0.35)),
                accent: light_accent,
            }
        }
        Theme::EditorialDark => {
            let a = parse_hex(&dark_accent).unwrap();
            Palette {
                canvas: "#0B0F17",
                card: "#151D2C",
                card_recessed: "#111827",
                border: "#223049",
                text: "#F8FAFC",
                muted: "#94A3B8",
                edge: "#4A5B77",
                edge_strong: "#CBD5E1",
                zone_fill: "#0F1521",
                zone_stroke: "#1E2A40",
                badge: "#101826",
                accent_tint: to_hex(mix((0x15, 0x1D, 0x2C), a, 0.14)),
                accent_soft: to_hex(mix((0x15, 0x1D, 0x2C), a, 0.45)),
                accent: dark_accent,
            }
        }
    }
}

pub fn theme_attr(theme: Theme) -> &'static str {
    match theme {
        Theme::EditorialLight => "editorial-light",
        Theme::EditorialDark => "editorial-dark",
    }
}

/// CSS custom properties for both themes under `selector[data-theme=…]`.
pub fn token_css(selector: &str, accent: &Accent) -> String {
    let mut css = String::new();
    for theme in [Theme::EditorialLight, Theme::EditorialDark] {
        let p = palette(theme, accent);
        css.push_str(&format!(
            "{selector}[data-theme=\"{}\"]{{--ad-canvas:{};--ad-card:{};--ad-card-recessed:{};--ad-border:{};--ad-text:{};--ad-muted:{};--ad-edge:{};--ad-edge-strong:{};--ad-zone-fill:{};--ad-zone-stroke:{};--ad-badge:{};--ad-accent:{};--ad-accent-tint:{};--ad-accent-soft:{};color-scheme:{}}}\n",
            theme_attr(theme),
            p.canvas, p.card, p.card_recessed, p.border, p.text, p.muted, p.edge, p.edge_strong, p.zone_fill, p.zone_stroke, p.badge, p.accent, p.accent_tint, p.accent_soft,
            if theme == Theme::EditorialLight { "light" } else { "dark" }
        ));
    }
    css
}

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn to_hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn mix(base: (u8, u8, u8), over: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let m = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
    (m(base.0, over.0), m(base.1, over.1), m(base.2, over.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_parse_and_tint() {
        assert_eq!(Accent::parse("Coral").unwrap(), Accent::Coral);
        assert_eq!(Accent::parse("#0ea5e9").unwrap(), Accent::Custom("#0EA5E9".into()));
        assert!(Accent::parse("rainbow").is_err());
        let p = palette(Theme::EditorialLight, &Accent::Indigo);
        assert_eq!(p.accent, "#4F46E5");
        assert_eq!(p.accent_tint, "#F4F4FD");
        let css = token_css(".nunki", &Accent::Coral);
        assert!(css.contains("--ad-accent:#E11D48") && css.contains("--ad-accent:#FB7185"));
    }
}
