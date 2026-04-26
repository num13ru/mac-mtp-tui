use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::types::{DeviceEntryKind, FocusPane};

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl App {
    pub fn draw(&self, frame: &mut Frame) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(frame.area());

        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vertical[0]);

        self.draw_host_pane(frame, panes[0]);
        self.draw_device_pane(frame, panes[1]);
        self.draw_status_bar(frame, vertical[1]);

        if self.confirm_dialog.is_some() {
            self.draw_confirm_dialog(frame);
        }

        if self.text_input_dialog.is_some() {
            self.draw_text_input_dialog(frame);
        }

        if self.info_dialog.is_some() {
            self.draw_info_dialog(frame);
        }

        if self.inspector.is_some() {
            self.draw_inspector(frame);
        }

        if self.show_help {
            self.draw_help(frame);
        }
    }

    fn draw_host_pane(&self, frame: &mut Frame, area: Rect) {
        let title = format!(" Host {} ", self.host_cwd.display());
        let block = pane_block(title, self.focus == FocusPane::Host);
        let items = self
            .host
            .entries
            .iter()
            .map(|entry| {
                let icon = if entry.is_dir { "📁" } else { "📄" };
                let size = entry
                    .size
                    .map(format_size)
                    .unwrap_or_else(|| "<DIR>".into());
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{} {}", icon, entry.name)),
                    Span::raw(format!("  {}", size)),
                ]))
            })
            .collect::<Vec<_>>();

        let mut state = ListState::default().with_selected(Some(self.host.selected));
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_device_pane(&self, frame: &mut Frame, area: Rect) {
        if self.backend.is_none() && !self.device_loading {
            let block = pane_block(
                " Device (not connected) ".into(),
                self.focus == FocusPane::Device,
            );
            let msg = self
                .device_error
                .as_deref()
                .unwrap_or("No MTP device found");
            let paragraph = Paragraph::new(msg).block(block).wrap(Wrap { trim: false });
            frame.render_widget(paragraph, area);
            return;
        }

        let title = if self.device_connecting {
            let spinner = SPINNER_FRAMES[self.spinner_tick % SPINNER_FRAMES.len()];
            format!(" Device (connecting…) {} ", spinner)
        } else if self.device_loading {
            let spinner = SPINNER_FRAMES[self.spinner_tick % SPINNER_FRAMES.len()];
            format!(
                " {} {} {} ",
                self.device_name_cached, self.device_path_cached, spinner,
            )
        } else {
            format!(" {} {} ", self.device_name_cached, self.device_path_cached)
        };

        let mut block = pane_block(title, self.focus == FocusPane::Device);
        if let Some((free, total)) = self.storage_info_cached {
            block = block.title_bottom(
                Line::from(format!(
                    " {} free / {} ",
                    format_size(free),
                    format_size(total)
                ))
                .alignment(Alignment::Right),
            );
        }

        if self.device_loading && self.device.entries.is_empty() {
            let msg = if self.device_connecting {
                "Connecting to device…".into()
            } else {
                match self.loading_progress {
                    Some((fetched, total)) if total > 0 => {
                        format!("Loading ({fetched}/{total})…")
                    }
                    _ => "Loading…".into(),
                }
            };
            let paragraph = Paragraph::new(msg).block(block).wrap(Wrap { trim: false });
            frame.render_widget(paragraph, area);
            return;
        }

        let items = self
            .device
            .entries
            .iter()
            .map(|entry| {
                let icon = if entry.kind == DeviceEntryKind::Directory {
                    "📁"
                } else {
                    "📚"
                };
                let size = entry
                    .size
                    .map(format_size)
                    .unwrap_or_else(|| "<DIR>".into());
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{} {}", icon, entry.name)),
                    Span::raw(format!("  {}", size)),
                ]))
            })
            .collect::<Vec<_>>();

        let mut state = ListState::default().with_selected(Some(self.device.selected));
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_status_bar(&self, frame: &mut Frame, area: Rect) {
        let text = format!(
            "Tab pane • i inspect • p push • g pull • d del • m mkdir • R rename • r refresh • ? help • q quit    {}",
            self.status
        );
        frame.render_widget(Paragraph::new(text), area);
    }

    fn draw_help(&self, frame: &mut Frame) {
        let area = centered_rect(frame.area(), 72, 55);
        frame.render_widget(Clear, area);

        let lines = vec![
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  Tab         switch active pane"),
            Line::from("  j / k       move selection"),
            Line::from("  Enter       enter directory"),
            Line::from("  Backspace   go to parent"),
            Line::from(""),
            Line::from("File actions (device pane):"),
            Line::from("  i           inspect object metadata"),
            Line::from("  p           push host file to device"),
            Line::from("  g           pull device file to host"),
            Line::from("  d           delete (confirms)"),
            Line::from("  m           create directory"),
            Line::from("  R           rename"),
            Line::from(""),
            Line::from("App:"),
            Line::from("  r           refresh both panes"),
            Line::from("  ?           toggle this help"),
            Line::from("  Esc         close dialog / help"),
            Line::from("  q           quit"),
        ];

        let help = Paragraph::new(lines)
            .block(Block::default().title(" Help ").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(help, area);
    }

    fn draw_inspector(&self, frame: &mut Frame) {
        let Some(data) = &self.inspector else {
            return;
        };

        let area = centered_rect(frame.area(), 75, 85);
        frame.render_widget(Clear, area);

        let dim = Style::default().add_modifier(Modifier::DIM);
        let bold = Style::default().add_modifier(Modifier::BOLD);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled("--- ObjectInfo ---", bold)),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Handle:      ", dim),
                Span::raw(&data.object_handle),
            ]),
            Line::from(vec![
                Span::styled("  Filename:    ", dim),
                Span::raw(&data.filename),
            ]),
            Line::from(vec![
                Span::styled("  Format:      ", dim),
                Span::raw(&data.format),
            ]),
            Line::from(vec![
                Span::styled("  Size:        ", dim),
                Span::raw(&data.size),
            ]),
            Line::from(vec![
                Span::styled("  Storage:     ", dim),
                Span::raw(&data.storage_id),
            ]),
            Line::from(vec![
                Span::styled("  Parent:      ", dim),
                Span::raw(&data.parent_id),
            ]),
            Line::from(vec![
                Span::styled("  Protection:  ", dim),
                Span::raw(&data.protection),
            ]),
            Line::from(vec![
                Span::styled("  Created:     ", dim),
                Span::raw(data.created.as_deref().unwrap_or("(none)")),
            ]),
            Line::from(vec![
                Span::styled("  Modified:    ", dim),
                Span::raw(data.modified.as_deref().unwrap_or("(none)")),
            ]),
        ];
        if !data.keywords.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Keywords:    ", dim),
                Span::raw(&data.keywords),
            ]));
        }
        if let Some(ref dims) = data.image_dimensions {
            lines.push(Line::from(vec![
                Span::styled("  Image dims:  ", dim),
                Span::raw(dims),
            ]));
        }
        if let Some(ref thumb) = data.thumb_dimensions {
            lines.push(Line::from(vec![
                Span::styled("  Thumbnail:   ", dim),
                Span::raw(thumb),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "--- MTP Properties (GetObjectPropValue) ---",
            bold,
        )));
        lines.push(Line::from(""));

        for prop in &data.properties {
            let label = format!("  0x{:04X} {:<20} ", prop.code, prop.name);
            let style = if prop.is_error { dim } else { Style::default() };
            let prefix = if prop.is_error { "ERR " } else { "" };
            lines.push(Line::from(vec![
                Span::styled(label, dim),
                Span::styled(format!("{prefix}{}", prop.value), style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  j/k scroll • Esc/i/q close",
            dim,
        )));

        let total_lines = lines.len();
        let inner_height = area.height.saturating_sub(2) as usize;
        let max_offset = total_lines.saturating_sub(inner_height);
        let offset = data.scroll_offset.min(max_offset);

        let visible: Vec<Line> = lines.into_iter().skip(offset).take(inner_height).collect();

        let title = format!(" Inspector: {} ", data.filename);
        let paragraph =
            Paragraph::new(visible).block(Block::default().title(title).borders(Borders::ALL));
        frame.render_widget(paragraph, area);
    }

    fn draw_text_input_dialog(&self, frame: &mut Frame) {
        let Some(dialog) = &self.text_input_dialog else {
            return;
        };

        let max_width = (frame.area().width).min(50);
        // borders (2) + blank + prompt + blank + input + blank + hint
        let height: u16 = 8;

        let area = centered_fixed(frame.area(), max_width, height);
        frame.render_widget(Clear, area);

        let inner_width = max_width.saturating_sub(2) as usize;
        let input = &dialog.input;
        let chars: Vec<(usize, char)> = input.char_indices().collect();
        let char_count = chars.len();

        let cursor_char = chars
            .iter()
            .position(|&(byte_pos, _)| byte_pos == dialog.cursor_pos)
            .unwrap_or(char_count);

        // Visible window: up to inner_width chars, kept so cursor is always in view.
        let vis_start = if cursor_char < inner_width {
            0
        } else {
            cursor_char - inner_width + 1
        };
        let vis_end = (vis_start + inner_width).min(char_count);
        let cursor_in_vis = cursor_char - vis_start;

        let mut before = String::new();
        let mut after = String::new();
        let mut cursor_ch: Option<char> = None;

        for (i, &(_, ch)) in chars
            .iter()
            .enumerate()
            .skip(vis_start)
            .take(vis_end - vis_start)
        {
            let rel = i - vis_start;
            if rel < cursor_in_vis {
                before.push(ch);
            } else if rel == cursor_in_vis {
                cursor_ch = Some(ch);
            } else {
                after.push(ch);
            }
        }

        let mut input_spans: Vec<Span> = Vec::new();
        if !before.is_empty() {
            input_spans.push(Span::raw(before));
        }
        if let Some(ch) = cursor_ch {
            input_spans.push(Span::styled(
                ch.to_string(),
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        } else {
            input_spans.push(Span::styled(
                " ",
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        }
        if !after.is_empty() {
            input_spans.push(Span::raw(after));
        }

        let lines = vec![
            Line::from(""),
            Line::from(Span::raw(&dialog.prompt)),
            Line::from(""),
            Line::from(input_spans),
            Line::from(""),
            Line::from(Span::raw("Enter confirm • Esc cancel")),
        ];

        let title = format!(" {} ", dialog.title);
        let paragraph = Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn draw_info_dialog(&self, frame: &mut Frame) {
        let Some(dialog) = &self.info_dialog else {
            return;
        };

        let max_width = (frame.area().width).min(55);
        let inner_width = max_width.saturating_sub(2);
        let msg_lines: u16 = dialog
            .message
            .lines()
            .map(|line| {
                if inner_width > 0 {
                    (line.len() as u16).div_ceil(inner_width)
                } else {
                    1
                }
                .max(1)
            })
            .sum();
        let height = 2 + 1 + msg_lines + 1 + 1;

        let area = centered_fixed(frame.area(), max_width, height);
        frame.render_widget(Clear, area);

        let mut lines: Vec<Line> = vec![Line::from("")];
        for text_line in dialog.message.lines() {
            lines.push(Line::from(Span::raw(text_line)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press any key to close",
            Style::default().add_modifier(Modifier::DIM),
        )));

        let title = format!(" {} ", dialog.title);
        let paragraph = Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn draw_confirm_dialog(&self, frame: &mut Frame) {
        let Some(dialog) = &self.confirm_dialog else {
            return;
        };

        let max_width = (frame.area().width).min(60);
        let inner_width = max_width.saturating_sub(2); // border left + right
        let msg_len = dialog.message.len() as u16;
        let msg_lines = if inner_width > 0 {
            msg_len.div_ceil(inner_width)
        } else {
            1
        };
        // borders top/bottom (2) + blank + message lines + blank + button line
        let height = 2 + 1 + msg_lines + 1 + 1;

        let area = centered_fixed(frame.area(), max_width, height);
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = vec![
            Line::from(""),
            Line::from(Span::raw(&dialog.message)),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Y]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("es    "),
                Span::styled("[N]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("o"),
            ]),
        ];

        let title = format!(" {} ", dialog.title);
        let paragraph = Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }
}

fn pane_block(title: String, active: bool) -> Block<'static> {
    let title = if active { format!(">{}", title) } else { title };

    Block::default().title(title).borders(Borders::ALL)
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn centered_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}
