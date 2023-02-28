use tui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph},
};

use super::{KeyHandler, Window};

pub(crate) struct Help {
    pub(crate) should_quit: bool,
}

impl KeyHandler for Help {
    fn is_focusing(&self, window_type: Window) -> bool {
        window_type == Window::Help
    }
    fn dimensions(&self) -> Option<(u16, u16)> {
        Some((100, 10))
    }
    fn widget(&self) -> Paragraph {
        let message = vec![
            Spans::from(Span::styled(
                "Are you sure you want to quit?",
                Style::default().fg(Color::White),
            )),
            Spans::from(Span::from("")),
            Spans::from(vec![
                Span::styled(
                    "    Yes    ",
                    if self.should_quit {
                        Style::default()
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
                Span::styled(
                    "    No    ",
                    if self.should_quit {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default()
                    },
                ),
            ]),
        ];
        Paragraph::new(message).alignment(Alignment::Center).block(
            Block::default()
                .title(Span::styled(
                    " Help ",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow)),
        )
    }
}

impl Help {
    pub(crate) fn new() -> Self {
        Self { should_quit: false }
    }
}
