//! In charge of calculating dimensions and displaying everything.

use std::{
    cmp,
    error::Error,
    io::{self, Stdout},
};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};

use crate::{
    app::{AppData, Nibble},
    label::{LabelHandler, LABEL_TITLES},
    windows::{editor::Editor, KeyHandler, Window},
};

use crate::byte::{as_str, get_color};

const COLOR_NULL: Color = Color::DarkGray;

pub(crate) struct ScreenHandler {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    pub(crate) terminal_size: Rect,
    pub(crate) comp_layouts: ComponentLayouts,
}

pub(crate) struct ComponentLayouts {
    line_numbers: Rect,
    pub(crate) hex: Rect,
    pub(crate) ascii: Rect,
    labels: Vec<Rect>,
    pub(crate) popup: Rect,
    pub(crate) bytes_per_line: usize,
    pub(crate) lines_per_screen: usize,
}

impl ScreenHandler {
    pub(crate) fn new() -> Result<Self, Box<dyn Error>> {
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        let terminal_size = terminal.size()?;
        Ok(Self {
            terminal,
            terminal_size,
            comp_layouts: Self::calculate_dimensions(terminal_size, &Editor::Hex),
        })
    }
    pub(crate) fn setup() -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(())
    }
    pub(crate) fn teardown(&mut self) -> Result<(), Box<dyn Error>> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
    pub(crate) fn identify_clicked_component(
        &self,
        row: u16,
        col: u16,
        window: &dyn KeyHandler,
    ) -> Window {
        let click = Rect::new(col, row, 0, 0);
        let popup_enabled = !(window.is_focusing(Window::Hex) || window.is_focusing(Window::Ascii));
        if popup_enabled && self.comp_layouts.popup.union(click) == self.comp_layouts.popup {
            return Window::Unhandled;
        } else if self.comp_layouts.hex.union(click) == self.comp_layouts.hex {
            return Window::Hex;
        } else if self.comp_layouts.ascii.union(click) == self.comp_layouts.ascii {
            return Window::Ascii;
        }
        for (i, &label) in self.comp_layouts.labels.iter().enumerate() {
            if label.union(click) == label {
                return Window::Label(i);
            }
        }
        Window::Unhandled
    }

    /// Calculates the dimensions of the components that will be continually displayed.
    ///
    /// This includes the editors, labels, and address table.
    fn calculate_dimensions(frame: Rect, window: &dyn KeyHandler) -> ComponentLayouts {
        // Establish Constraints
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(12)])
            .split(frame);
        let editors = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(10),
                // The address table is Length(10) as specified above. Because the hex editor takes
                // 3 graphemes for every 1 that ASCII takes (each nibble plus a space), we multiply
                // the editors by those ratios.
                Constraint::Length((frame.width - 10) * 3 / 4 - 1),
                Constraint::Length((frame.width - 10) / 4),
            ])
            .split(sections[0]);
        let mut labels = Vec::with_capacity(12);
        let label_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 4),
                Constraint::Ratio(1, 4),
                Constraint::Ratio(1, 4),
                Constraint::Ratio(1, 4),
            ])
            .split(sections[1]);
        for label in label_cols {
            labels.append(
                &mut Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Ratio(1, 4),
                        Constraint::Ratio(1, 4),
                        Constraint::Ratio(1, 4),
                        Constraint::Ratio(1, 4),
                    ])
                    .split(label),
            );
        }

        // Calculate popup dimensions
        let popup = Self::calculate_popup_dimensions(frame, window);

        // Calculate bytes per line
        let bytes_per_line = ((editors[1].width - 2) / 3) as usize;
        let lines_per_screen = (editors[1].height - 2) as usize;

        ComponentLayouts {
            line_numbers: editors[0],
            hex: editors[1],
            ascii: editors[2],
            popup,
            bytes_per_line,
            lines_per_screen,
            labels,
        }
    }

    /// Calculates the dimensions of the popup that is being focused. Currently used in
    /// [`calculate_dimensions`](Self::calculate_dimensions) and
    /// [`set_focused_window`](crate::app::Application::set_focused_window)
    /// since the dimensions are constant and are only changed when the popup changes.
    pub(crate) fn calculate_popup_dimensions(frame: Rect, window: &dyn KeyHandler) -> Rect {
        window.dimensions().map_or_else(Rect::default, |dimensions| popup_rect(dimensions, frame))
    }

    /// Generates all the visuals of the file contents to be displayed to user by calling
    /// [`generate_hex`](generate_hex) and [`generate_ascii`](generate_ascii).
    fn generate_text(
        app_info: &mut AppData,
        bytes_per_line: usize,
        lines_per_screen: usize,
    ) -> (Text, Text, Text) {
        let content_lines = app_info.contents.len() / bytes_per_line + 1;
        let start_row = app_info.start_address / bytes_per_line;

        // Generate address lines
        let address_text = (0..cmp::min(lines_per_screen, content_lines - start_row))
            .map(|i| {
                let row_address = app_info.start_address + i * bytes_per_line;
                let mut span = Span::from(format!("{:08X?}\n", row_address));
                // Highlight the address row that the cursor is in for visibility
                if (row_address..row_address + bytes_per_line).contains(&app_info.offset) {
                    span.style = span.style.fg(Color::Black).bg(Color::White);
                }
                Spans::from(span)
            })
            .collect::<Vec<Spans>>();

        let hex_text = generate_hex(app_info, bytes_per_line, lines_per_screen);

        let ascii_text = generate_ascii(app_info, bytes_per_line, lines_per_screen);

        (address_text.into(), hex_text.into(), ascii_text.into())
    }

    /// Display the addresses, editors, labels, and popups based off of the specifications of
    /// [`ComponentLayouts`], defined by
    /// [`calculate_dimensions`](Self::calculate_dimensions).
    pub(crate) fn render(
        &mut self,
        app_info: &mut AppData,
        labels: &LabelHandler,
        window: &dyn KeyHandler,
    ) -> Result<(), Box<dyn Error>> {
        self.terminal.draw(|f| {
            // We check if we need to recompute the terminal size in the case that the saved off
            // variable differs from the current frame, which can occur when a terminal is resized
            // between an event handling and a rendering.
            let size = f.size();
            if size != self.terminal_size {
                self.terminal_size = size;
                self.comp_layouts = Self::calculate_dimensions(self.terminal_size, window);

                // We change the start_address here to ensure that 0 is ALWAYS the first start
                // address. We round to preventing constant resizing always moving to 0.
                app_info.start_address = (app_info.start_address
                    + (self.comp_layouts.bytes_per_line / 2))
                    / self.comp_layouts.bytes_per_line
                    * self.comp_layouts.bytes_per_line;
            }

            // Check if terminal is large enough
            if self.terminal_size.width < 50 || self.terminal_size.height < 15 {
                let dimension_notification = Paragraph::new("Terminal dimensions must be larger!")
                    .block(Block::default())
                    .alignment(Alignment::Center);
                let vertical_center = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(40),
                        Constraint::Percentage(20),
                        Constraint::Percentage(40),
                    ])
                    .split(self.terminal_size);
                f.render_widget(dimension_notification, vertical_center[1]);
                return;
            }

            let (address_text, hex_text, ascii_text) = Self::generate_text(
                app_info,
                self.comp_layouts.bytes_per_line,
                self.comp_layouts.lines_per_screen,
            );

            // Render Line Numbers
            f.render_widget(
                Paragraph::new(address_text)
                    .block(Block::default().borders(Borders::ALL).title("Address")),
                self.comp_layouts.line_numbers,
            );

            // Render Hex
            f.render_widget(
                Paragraph::new(hex_text).block(
                    Block::default().borders(Borders::ALL).title("Hex").style(
                        if window.is_focusing(Window::Hex) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        },
                    ),
                ),
                self.comp_layouts.hex,
            );

            // Render ASCII
            f.render_widget(
                Paragraph::new(ascii_text).block(
                    Block::default().borders(Borders::ALL).title("ASCII").style(
                        if window.is_focusing(Window::Ascii) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        },
                    ),
                ),
                self.comp_layouts.ascii,
            );

            // Render Info
            for (i, label) in self.comp_layouts.labels.iter().enumerate() {
                f.render_widget(
                    Paragraph::new(labels[LABEL_TITLES[i]].clone())
                        .block(Block::default().borders(Borders::ALL).title(LABEL_TITLES[i])),
                    *label,
                );
            }

            // Render Popup
            if !window.is_focusing(Window::Hex) && !window.is_focusing(Window::Ascii) {
                f.render_widget(Clear, self.comp_layouts.popup);
                f.render_widget(window.widget(), self.comp_layouts.popup);
            }
        })?;
        Ok(())
    }
}

/// Display hex bytes with correct highlighting and colors by chunking the bytes into rows and
/// formatting them into hex.
fn generate_hex(app_info: &AppData, bytes_per_line: usize, lines_per_screen: usize) -> Vec<Spans> {
    app_info.contents[app_info.start_address..]
        .chunks(bytes_per_line)
        .take(lines_per_screen)
        .enumerate()
        .map(|(row, chunk)| {
            let spans = chunk
                .iter()
                .enumerate()
                .flat_map(|(col, &byte)| {
                    // We don't want an extra space at the end of each row.
                    if col < bytes_per_line - 1 {
                        format!("{byte:02X?} ")
                    } else {
                        format!("{byte:02X?}")
                    }
                    .chars()
                    .enumerate()
                    .map(|(nibble_pos, c)| {
                        let byte_pos = app_info.start_address + (row * bytes_per_line) + col;
                        let mut span =
                            Span::styled(c.to_string(), Style::default().fg(*get_color(byte)));
                        let is_cursor = byte_pos == app_info.offset
                            && ((nibble_pos == 0 && app_info.nibble == Nibble::Beginning)
                                || (nibble_pos == 1 && app_info.nibble == Nibble::End));

                        // Determine if the specified nibble (or space) should have a
                        // lighter foreground because it is in the user's dragged range.
                        // The logic is more complicated for hex because users can select
                        // a single nibble from a byte.
                        let mut in_drag = false;
                        if let Some(drag) = app_info.last_drag {
                            let drag_nibble = app_info.drag_nibble.unwrap_or(Nibble::End);
                            if !(drag == app_info.offset && app_info.nibble == drag_nibble) {
                                let mut start = drag;
                                let mut end = app_info.offset;
                                let mut start_nibble = drag_nibble;
                                let mut end_nibble = app_info.nibble;

                                if app_info.offset < drag {
                                    start = app_info.offset;
                                    end = drag;
                                    start_nibble = app_info.nibble;
                                    end_nibble = drag_nibble;
                                }

                                // The only time the starting byte would not entirely be in
                                // drag range is when the first nibble is not highlighted.
                                // Similarly, the last nibble is only partially highlighted
                                // when the second (and last) nibble is not selected.
                                if byte_pos == start {
                                    in_drag = !(nibble_pos == 0 && start_nibble == Nibble::End);
                                }
                                if byte_pos == end {
                                    in_drag |= !(nibble_pos == 1
                                        && end_nibble == Nibble::Beginning)
                                        && nibble_pos != 2;
                                }
                                if start == end && nibble_pos == 2 {
                                    in_drag = false;
                                } else if end - start > 1 {
                                    in_drag |= (start + 1..end).contains(&byte_pos);
                                }
                            }
                        }
                        if is_cursor || in_drag {
                            span.style = span.style.bg(COLOR_NULL);
                        }
                        span
                    })
                    .collect::<Vec<Span>>()
                })
                .collect::<Vec<Span>>();
            Spans::from(spans)
        })
        .collect::<Vec<Spans>>()
}

/// Display ASCII bytes with correct highlighting and colors.
fn generate_ascii(
    app_info: &AppData,
    bytes_per_line: usize,
    lines_per_screen: usize,
) -> Vec<Spans> {
    app_info.contents[app_info.start_address..]
        .chunks(bytes_per_line)
        .take(lines_per_screen)
        .enumerate()
        .map(|(row, chunk)| {
            Spans::from(
                chunk
                    .iter()
                    .enumerate()
                    .map(|(col, &byte)| {
                        let byte_pos = app_info.start_address + (row * bytes_per_line) + col;
                        let mut span =
                            Span::styled(as_str(byte), Style::default().fg(*get_color(byte)));
                        // Highlight the selected byte in the ASCII table
                        let last_drag = app_info.last_drag.unwrap_or(app_info.offset);
                        if byte_pos == app_info.offset
                            || (app_info.offset..=last_drag).contains(&byte_pos)
                            || (last_drag..=app_info.offset).contains(&byte_pos)
                        {
                            span.style = span.style.bg(COLOR_NULL);
                        }
                        span
                    })
                    .collect::<Vec<Span>>(),
            )
        })
        .collect::<Vec<Spans>>()
}

/// Generates the dimensions of an x by y popup that is centered in Rect r.
fn popup_rect((x, y): (u16, u16), r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(r.height.saturating_sub(y) / 2),
                Constraint::Length(y),
                Constraint::Min(r.height.saturating_sub(y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Min(r.width.saturating_sub(x) / 2),
                Constraint::Length(x),
                Constraint::Min(r.width.saturating_sub(x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
