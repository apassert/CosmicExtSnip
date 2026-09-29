//! What the editor remembers between snips: the colour and the stroke widths.
//! The tool is deliberately not remembered; every snip starts with the pen.

use std::path::PathBuf;

use crate::config;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prefs {
    /// Index into `config::PALETTE`.
    pub color: usize,
    pub pen_width: f32,
    pub highlight_width: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            color: 0,
            pen_width: config::DEFAULT_PEN_WIDTH,
            highlight_width: config::DEFAULT_HIGHLIGHT_WIDTH,
        }
    }
}

impl Prefs {
    /// Reads `key=value` lines. Anything unknown, malformed or out of range
    /// falls back to the default for that key, so a damaged file never stops
    /// the editor from opening.
    pub fn parse(text: &str) -> Prefs {
        let mut prefs = Prefs::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "color" => {
                    if let Some(i) = config::PALETTE.iter().position(|c| value == c.name) {
                        prefs.color = i;
                    }
                }
                "pen_width" => {
                    if let Ok(w) = value.parse::<f32>()
                        && w.is_finite()
                    {
                        prefs.pen_width = w.clamp(config::PEN_WIDTH_MIN, config::PEN_WIDTH_MAX);
                    }
                }
                "highlight_width" => {
                    if let Ok(w) = value.parse::<f32>()
                        && w.is_finite()
                    {
                        prefs.highlight_width =
                            w.clamp(config::HIGHLIGHT_WIDTH_MIN, config::HIGHLIGHT_WIDTH_MAX);
                    }
                }
                _ => {}
            }
        }
        prefs
    }

    pub fn format(&self) -> String {
        format!(
            "color={}\npen_width={}\nhighlight_width={}\n",
            config::PALETTE[self.color.min(config::PALETTE.len() - 1)].name,
            self.pen_width,
            self.highlight_width
        )
    }

    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("cosmicsnip").join("editor.conf"))
    }

    pub fn load() -> Prefs {
        Prefs::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Prefs::parse(&t))
            .unwrap_or_default()
    }

    /// Writes through a temporary file so a crash never leaves half a file.
    pub fn store(&self) -> Result<(), String> {
        let path = Prefs::path().ok_or("no config directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, self.format())
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_is_read_back() {
        let prefs = Prefs {
            color: 2,
            pen_width: 7.0,
            highlight_width: 32.0,
        };
        assert_eq!(Prefs::parse(&prefs.format()), prefs);
    }

    #[test]
    fn the_colour_is_stored_by_name() {
        let prefs = Prefs {
            color: 3,
            ..Prefs::default()
        };
        assert!(
            prefs
                .format()
                .contains(&format!("color={}", config::PALETTE[3].name))
        );
    }

    #[test]
    fn a_damaged_file_falls_back_per_key() {
        let prefs = Prefs::parse("color=purple\npen_width=abc\nhighlight_width=NaN\nnoise\n");
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn widths_out_of_range_are_clamped() {
        let prefs = Prefs::parse("pen_width=500\nhighlight_width=0\n");
        assert_eq!(prefs.pen_width, config::PEN_WIDTH_MAX);
        assert_eq!(prefs.highlight_width, config::HIGHLIGHT_WIDTH_MIN);
    }
}
