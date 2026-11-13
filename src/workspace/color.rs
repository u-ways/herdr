//! Workspace accent colors.
//!
//! A workspace color is an sRGB triple stored and sent as lowercase `#rrggbb`.
//! Users pick one of the fixed light swatches or give any hex value.

use std::path::{Component, Path, PathBuf};

use ratatui::style::Color;

use crate::app::state::Palette;
use crate::config::WorkspaceColorRule;
use crate::terminal_theme::RgbColor;

/// How far a background tint moves from the terminal background toward the color.
const TINT_ALPHA: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkspaceColor {
    r: u8,
    g: u8,
    b: u8,
}

/// Light pastel swatches (OKLCH L=0.80, C=0.11) with evenly spread hues, in menu order.
pub const SWATCHES: [(&str, WorkspaceColor); 8] = [
    ("rose", swatch(0xfca0a7)),
    ("orange", swatch(0xf2ad73)),
    ("yellow", swatch(0xcec068)),
    ("green", swatch(0x90d192)),
    ("teal", swatch(0x59d4cd)),
    ("sky", swatch(0x71c9fa)),
    ("violet", swatch(0xbcb2ff)),
    ("pink", swatch(0xeea2d4)),
];

const fn swatch(hex: u32) -> WorkspaceColor {
    WorkspaceColor {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

impl WorkspaceColor {
    /// Parses `#rrggbb`, `rrggbb`, `#rgb`, or a swatch name, ignoring case and
    /// surrounding whitespace. Does not allocate.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(&(_, color)) = SWATCHES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(value))
        {
            return Some(color);
        }
        let (digits, short_form) = match value.strip_prefix('#') {
            Some(digits) => (digits, true),
            None => (value, false),
        };
        match *digits.as_bytes() {
            [r1, r2, g1, g2, b1, b2] => Some(Self {
                r: hex_byte(r1, r2)?,
                g: hex_byte(g1, g2)?,
                b: hex_byte(b1, b2)?,
            }),
            [r, g, b] if short_form => Some(Self {
                r: hex_byte(r, r)?,
                g: hex_byte(g, g)?,
                b: hex_byte(b, b)?,
            }),
            _ => None,
        }
    }

    /// Lowercase `#rrggbb`, the stored and wire form.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn rgb(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    pub fn to_ratatui(self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }

    /// Black or white, whichever reads better as text drawn on this color.
    pub fn contrast_fg(self) -> Color {
        crate::ui::contrast_fg_for_rgb(self.rgb())
    }

    /// This color drawn as text or a glyph on `palette`'s background,
    /// darkened on light palettes where the light swatches wash out.
    pub fn foreground(self, palette: &Palette) -> Color {
        crate::ui::foreground_on_palette(self.rgb(), palette)
    }

    /// A faint shade of this color to fill backgrounds with: the host terminal
    /// background, else the palette's RGB panel background, nudged toward it.
    /// `None` when neither is known, as with the `terminal` theme.
    pub fn tint(self, host_background: Option<RgbColor>, palette: &Palette) -> Option<Color> {
        let base = match (host_background, palette.panel_bg) {
            (Some(host), _) => (host.r, host.g, host.b),
            (None, Color::Rgb(r, g, b)) => (r, g, b),
            (None, _) => return None,
        };
        let (r, g, b) = crate::ui::mix_rgb(base, self.rgb(), TINT_ALPHA);
        Some(Color::Rgb(r, g, b))
    }
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let high = char::from(high).to_digit(16)?;
    let low = char::from(low).to_digit(16)?;
    u8::try_from(high * 16 + low).ok()
}

/// Picks the swatch that `others` use least, never `exclude`. Ties go to the
/// first such swatch in palette order, or to a `seed`-chosen one.
pub fn least_used(
    others: impl IntoIterator<Item = WorkspaceColor>,
    exclude: Option<WorkspaceColor>,
    seed: Option<u64>,
) -> WorkspaceColor {
    let mut uses = [0usize; SWATCHES.len()];
    for color in others {
        if let Some(index) = SWATCHES.iter().position(|&(_, swatch)| swatch == color) {
            uses[index] += 1;
        }
    }
    let candidates = || {
        SWATCHES
            .iter()
            .zip(uses)
            .filter(|&(&(_, swatch), _)| Some(swatch) != exclude)
    };
    let fewest = candidates().map(|(_, count)| count).min().unwrap_or(0);
    let tied = || {
        candidates()
            .filter(move |&(_, count)| count == fewest)
            .map(|(&(_, swatch), _)| swatch)
    };
    let pick = seed.map_or(0, |seed| seed % tied().count().max(1) as u64);
    tied().nth(pick as usize).unwrap_or(SWATCHES[0].1)
}

/// `[ui.workspace_colors]` path rules, resolved once per config load so a
/// lookup touches no filesystem and allocates nothing. Most specific first.
#[derive(Debug, Default)]
pub struct WorkspaceColorRules(Vec<(PathBuf, WorkspaceColor, Option<bool>)>);

impl WorkspaceColorRules {
    /// Keeps the valid rules, with `~` expanded and paths lexically
    /// normalized, and describes every rule it ignores. A rule whose path
    /// goes through a symlink also matches the path it resolves to.
    pub fn resolve(rules: &[WorkspaceColorRule]) -> (Self, Vec<String>) {
        let mut resolved = Vec::new();
        let mut diagnostics = Vec::new();
        for (index, rule) in rules.iter().enumerate() {
            let path = crate::worktree::expand_tilde_path(&rule.path);
            if !path.is_absolute() {
                diagnostics.push(format!(
                    "ui.workspace_colors.rules[{index}] path {:?} must be absolute or start with ~; ignoring rule",
                    rule.path
                ));
            }
            let color = WorkspaceColor::parse(&rule.color);
            if color.is_none() {
                diagnostics.push(format!(
                    "ui.workspace_colors.rules[{index}] color {:?} is not #rrggbb, #rgb, or one of: {}; ignoring rule",
                    rule.color,
                    SWATCHES.map(|(name, _)| name).join(", ")
                ));
            }
            if let Some(color) = color.filter(|_| path.is_absolute()) {
                let path = normalize_lexically(&path);
                // Process working directories have their symlinks resolved.
                let physical = crate::worktree::canonical_or_original(&path);
                if physical != path {
                    resolved.push((physical, color, rule.background));
                }
                resolved.push((path, color, rule.background));
            }
        }
        // Stable, so the first of two identical paths wins.
        resolved.sort_by_key(|(path, ..)| std::cmp::Reverse(path.components().count()));
        (Self(resolved), diagnostics)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The color and tint choice of the most specific rule containing `cwd`.
    /// Paths compare by whole components: `/a/b` does not contain `/a/bc`.
    pub fn matching(&self, cwd: &Path) -> Option<(WorkspaceColor, Option<bool>)> {
        self.0
            .iter()
            .find(|(path, ..)| cwd.starts_with(path))
            .map(|&(_, color, background)| (color, background))
    }
}

/// Drops `.` and resolves `..` without touching the filesystem.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> WorkspaceColor {
        SWATCHES
            .iter()
            .find(|(swatch_name, _)| *swatch_name == name)
            .map(|&(_, color)| color)
            .unwrap()
    }

    #[test]
    fn parse_accepts_hex_forms_and_swatch_names() {
        let teal = Some(named("teal"));
        assert_eq!(WorkspaceColor::parse("#59d4cd"), teal);
        assert_eq!(WorkspaceColor::parse("59D4CD"), teal);
        assert_eq!(WorkspaceColor::parse("  #59D4cd\n"), teal);
        assert_eq!(WorkspaceColor::parse("teal"), teal);
        assert_eq!(WorkspaceColor::parse(" TeAl "), teal);
        assert_eq!(
            WorkspaceColor::parse("#F0a").map(WorkspaceColor::rgb),
            Some((0xff, 0x00, 0xaa))
        );
    }

    #[test]
    fn parse_rejects_invalid_input() {
        for invalid in [
            "",
            " ",
            "#",
            "f0a",
            "#f0",
            "#f0a0",
            "#f0a0a0a",
            "#gggggg",
            "#+f+f+f",
            "##f0a0a0",
            "#ééé",
            "red",
            "cyan",
            "random",
            "none",
            "rgb(1,2,3)",
        ] {
            assert_eq!(WorkspaceColor::parse(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn hex_round_trips_in_lowercase() {
        let color = WorkspaceColor::parse("#ABCDEF").unwrap();
        assert_eq!(color.to_hex(), "#abcdef");
        assert_eq!(WorkspaceColor::parse(&color.to_hex()), Some(color));
        assert_eq!(color.to_ratatui(), Color::Rgb(0xab, 0xcd, 0xef));
        for (_, swatch) in SWATCHES {
            assert_eq!(WorkspaceColor::parse(&swatch.to_hex()), Some(swatch));
        }
    }

    #[test]
    fn swatches_have_the_documented_names_and_values() {
        let swatches: Vec<_> = SWATCHES
            .iter()
            .map(|&(name, color)| (name, color.to_hex()))
            .collect();
        assert_eq!(
            swatches,
            [
                ("rose", "#fca0a7"),
                ("orange", "#f2ad73"),
                ("yellow", "#cec068"),
                ("green", "#90d192"),
                ("teal", "#59d4cd"),
                ("sky", "#71c9fa"),
                ("violet", "#bcb2ff"),
                ("pink", "#eea2d4"),
            ]
            .map(|(name, hex)| (name, hex.to_string()))
        );
    }

    #[test]
    fn contrast_fg_picks_black_on_light_swatches_and_white_on_dark_colors() {
        for (_, swatch) in SWATCHES {
            assert_eq!(swatch.contrast_fg(), Color::Rgb(0, 0, 0));
        }
        let navy = WorkspaceColor::parse("#101040").unwrap();
        assert_eq!(navy.contrast_fg(), Color::Rgb(255, 255, 255));
    }

    #[test]
    fn tint_mixes_slightly_from_the_host_background_else_the_panel_background() {
        let teal = named("teal");
        let host = RgbColor {
            r: 30,
            g: 30,
            b: 46,
        };
        let palette = Palette::catppuccin();
        assert_eq!(
            teal.tint(Some(host), &palette),
            Some(Color::Rgb(37, 52, 65))
        );
        // Without a host report the RGB panel background (24, 24, 37) is the base.
        assert_eq!(teal.tint(None, &palette), Some(Color::Rgb(32, 47, 57)));
        // The `terminal` theme has no RGB background to mix from.
        assert_eq!(teal.tint(None, &Palette::terminal()), None);
        assert_eq!(
            teal.tint(Some(host), &Palette::terminal()),
            Some(Color::Rgb(37, 52, 65))
        );
    }

    #[test]
    fn least_used_prefers_unused_swatches_in_palette_order() {
        assert_eq!(least_used([], None, None), named("rose"));
        assert_eq!(
            least_used([named("rose"), named("orange")], None, None),
            named("yellow")
        );
        // Custom colors do not count against any swatch.
        let custom = WorkspaceColor::parse("#123456").unwrap();
        assert_eq!(least_used([custom], None, None), named("rose"));
    }

    #[test]
    fn least_used_never_returns_the_excluded_color() {
        assert_eq!(least_used([], Some(named("rose")), None), named("orange"));
        // The current color is skipped even when it is the only unused swatch.
        let others: Vec<_> = SWATCHES[1..].iter().map(|&(_, color)| color).collect();
        assert_eq!(
            least_used(others.clone(), Some(named("rose")), None),
            named("orange")
        );
        for seed in 0..32 {
            assert_ne!(
                least_used(others.clone(), Some(named("rose")), Some(seed)),
                named("rose")
            );
        }
    }

    #[test]
    fn least_used_wraps_around_once_every_swatch_is_used() {
        let mut others: Vec<_> = SWATCHES.iter().map(|&(_, color)| color).collect();
        assert_eq!(least_used(others.clone(), None, None), named("rose"));
        others.push(named("rose"));
        others.push(named("orange"));
        assert_eq!(least_used(others, None, None), named("yellow"));
    }

    #[test]
    fn least_used_seed_breaks_ties_only_among_the_least_used() {
        let others = [named("rose"), named("orange"), named("yellow")];
        let tied = [
            named("green"),
            named("teal"),
            named("sky"),
            named("violet"),
            named("pink"),
        ];
        for seed in 0..10u64 {
            let picked = least_used(others, None, Some(seed));
            assert_eq!(picked, tied[(seed % 5) as usize]);
        }
    }

    fn rules(rules: &[(&str, &str, Option<bool>)]) -> (WorkspaceColorRules, Vec<String>) {
        let rules: Vec<_> = rules
            .iter()
            .map(|&(path, color, background)| WorkspaceColorRule {
                path: path.into(),
                color: color.into(),
                background,
            })
            .collect();
        WorkspaceColorRules::resolve(&rules)
    }

    /// An absolute directory on every platform, as a rule path string.
    fn absolute(relative: &str) -> String {
        std::env::temp_dir().join(relative).display().to_string()
    }

    #[test]
    fn color_rules_match_whole_components_and_prefer_the_most_specific_path() {
        let (rules, diagnostics) = rules(&[
            (&absolute("mns/api/adept"), "rose", Some(true)),
            (&absolute("mns"), "green", None),
            (&format!("{}/", absolute("mns/api")), "sky", Some(false)),
        ]);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let matching = |cwd: &str| rules.matching(Path::new(&absolute(cwd)));

        assert_eq!(matching("mns"), Some((named("green"), None)));
        assert_eq!(matching("mns/web/src"), Some((named("green"), None)));
        assert_eq!(matching("mns/api"), Some((named("sky"), Some(false))));
        assert_eq!(
            matching("mns/api/adept/src"),
            Some((named("rose"), Some(true)))
        );
        assert_eq!(matching("mns-old"), None);
        assert_eq!(matching("mnsx/api"), None);
        assert_eq!(matching(""), None);
        assert_eq!(
            WorkspaceColorRules::default().matching(Path::new(&absolute("mns"))),
            None
        );
    }

    #[test]
    fn color_rules_expand_home_and_normalize_paths_lexically() {
        let home = crate::worktree::expand_tilde_path("~");
        assert!(home.is_absolute(), "tests need a home directory");
        let (rules, diagnostics) = rules(&[
            ("~/github/mns/", "green", None),
            (
                &format!("{}/./apps/../web//", absolute("srv")),
                "#f38ba8",
                None,
            ),
            ("~", "violet", None),
        ]);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        assert_eq!(
            rules.matching(&home.join("github").join("mns").join("api")),
            Some((named("green"), None))
        );
        assert_eq!(
            rules.matching(&home.join("notes")),
            Some((named("violet"), None))
        );
        assert_eq!(
            rules.matching(Path::new(&absolute("srv/web/site"))),
            WorkspaceColor::parse("#f38ba8").map(|color| (color, None))
        );
        assert_eq!(rules.matching(Path::new(&absolute("srv/apps/web"))), None);
    }

    #[cfg(unix)]
    #[test]
    fn color_rules_through_a_symlink_also_match_its_target() {
        let root =
            std::env::temp_dir().join(format!("herdr-color-rule-symlink-{}", std::process::id()));
        let real = root.join("real");
        let link = root.join("link");
        std::fs::create_dir_all(real.join("proj")).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let (rules, diagnostics) = rules(&[(&link.display().to_string(), "green", None)]);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        let green = Some((named("green"), None));
        // `--cwd` through the link gives the logical path; a shell reports the physical one.
        assert_eq!(rules.matching(&link.join("proj")), green);
        let physical = std::fs::canonicalize(real.join("proj")).unwrap();
        assert_eq!(rules.matching(&physical), green);
        assert_eq!(rules.matching(&root), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn color_rules_drop_invalid_rules_and_name_them() {
        let (rules, diagnostics) = rules(&[
            ("relative/dir", "green", None),
            ("~someone/dir", "green", None),
            (&absolute("api"), "cyan", None),
            ("", "", None),
            (&absolute("web"), "teal", None),
        ]);

        assert_eq!(
            rules.matching(Path::new(&absolute("web"))),
            Some((named("teal"), None))
        );
        assert_eq!(rules.matching(Path::new(&absolute("api"))), None);
        let swatches = "rose, orange, yellow, green, teal, sky, violet, pink";
        assert_eq!(
            diagnostics,
            [
                "ui.workspace_colors.rules[0] path \"relative/dir\" must be absolute or start with ~; ignoring rule".to_string(),
                "ui.workspace_colors.rules[1] path \"~someone/dir\" must be absolute or start with ~; ignoring rule".to_string(),
                format!("ui.workspace_colors.rules[2] color \"cyan\" is not #rrggbb, #rgb, or one of: {swatches}; ignoring rule"),
                "ui.workspace_colors.rules[3] path \"\" must be absolute or start with ~; ignoring rule".to_string(),
                format!("ui.workspace_colors.rules[3] color \"\" is not #rrggbb, #rgb, or one of: {swatches}; ignoring rule"),
            ]
        );
    }
}
