use ratatui::style::{Color, Modifier, Style};

/// A colour scheme. Three ship with trafford; `night` is the default.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub panel: Color,
    pub fg: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub link: Color,
    pub link_broken: Color,
    pub heading: Color,
    pub code: Color,
    pub tag: Color,
    pub add: Color,
    pub del: Color,
    pub sel: Color,
    pub cursorline: Color,
    pub border: Color,
    pub border_focus: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::night()
    }
}

impl Theme {
    pub fn named(name: &str) -> Theme {
        match name {
            "paper" => Theme::paper(),
            "mono" => Theme::mono(),
            _ => Theme::night(),
        }
    }

    /// Warm ink-on-dark: the default.
    pub fn night() -> Theme {
        Theme {
            bg: Color::Rgb(0x14, 0x13, 0x1a),
            panel: Color::Rgb(0x1a, 0x19, 0x22),
            fg: Color::Rgb(0xd6, 0xd1, 0xc4),
            dim: Color::Rgb(0x8b, 0x85, 0x97),
            faint: Color::Rgb(0x4d, 0x49, 0x59),
            accent: Color::Rgb(0xe0, 0xa4, 0x58),
            link: Color::Rgb(0xa5, 0x8c, 0xe0),
            link_broken: Color::Rgb(0xc4, 0x6a, 0x7c),
            heading: Color::Rgb(0xf0, 0xeb, 0xdd),
            code: Color::Rgb(0x7f, 0xb3, 0xa8),
            tag: Color::Rgb(0x6f, 0xa8, 0xc4),
            add: Color::Rgb(0x86, 0xb0, 0x6e),
            del: Color::Rgb(0xd1, 0x68, 0x7a),
            sel: Color::Rgb(0x2e, 0x2b, 0x3d),
            cursorline: Color::Rgb(0x1f, 0x1e, 0x28),
            border: Color::Rgb(0x2e, 0x2b, 0x38),
            border_focus: Color::Rgb(0x5a, 0x52, 0x70),
        }
    }

    /// Light, for daylight and for terminals with pale backgrounds.
    pub fn paper() -> Theme {
        Theme {
            bg: Color::Rgb(0xf6, 0xf2, 0xe9),
            panel: Color::Rgb(0xed, 0xe8, 0xdc),
            fg: Color::Rgb(0x2e, 0x2a, 0x24),
            dim: Color::Rgb(0x6e, 0x67, 0x5c),
            faint: Color::Rgb(0xa8, 0xa0, 0x93),
            accent: Color::Rgb(0xa8, 0x62, 0x1b),
            link: Color::Rgb(0x6b, 0x4f, 0xa8),
            link_broken: Color::Rgb(0xb0, 0x3d, 0x53),
            heading: Color::Rgb(0x18, 0x15, 0x11),
            code: Color::Rgb(0x2f, 0x6f, 0x63),
            tag: Color::Rgb(0x2b, 0x5f, 0x82),
            add: Color::Rgb(0x3f, 0x7a, 0x33),
            del: Color::Rgb(0xa8, 0x35, 0x45),
            sel: Color::Rgb(0xdd, 0xd6, 0xc6),
            cursorline: Color::Rgb(0xec, 0xe6, 0xd8),
            border: Color::Rgb(0xd2, 0xca, 0xba),
            border_focus: Color::Rgb(0x9a, 0x8f, 0x7c),
        }
    }

    /// Uses the terminal's own palette — good over ssh and in odd terminals.
    pub fn mono() -> Theme {
        Theme {
            bg: Color::Reset,
            panel: Color::Reset,
            fg: Color::Reset,
            dim: Color::DarkGray,
            faint: Color::DarkGray,
            accent: Color::Yellow,
            link: Color::Magenta,
            link_broken: Color::Red,
            heading: Color::White,
            code: Color::Cyan,
            tag: Color::Blue,
            add: Color::Green,
            del: Color::Red,
            sel: Color::DarkGray,
            cursorline: Color::Reset,
            border: Color::DarkGray,
            border_focus: Color::Gray,
        }
    }

    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn dimmed(&self) -> Style {
        Style::default().fg(self.dim)
    }

    pub fn faded(&self) -> Style {
        Style::default().fg(self.faint)
    }

    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        Style::default()
            .fg(self.heading)
            .bg(self.sel)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        Style::default().fg(if focused {
            self.border_focus
        } else {
            self.border
        })
    }

    /// Heading colour ramps from bright to dim as the level deepens.
    pub fn heading_style(&self, level: u8) -> Style {
        let base = Style::default().add_modifier(Modifier::BOLD);
        match level {
            1 => base.fg(self.accent),
            2 => base.fg(self.heading),
            3 => base.fg(self.fg),
            _ => base.fg(self.dim),
        }
    }
}
