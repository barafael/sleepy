//! Drawing: one function, one frame, from the app's state to the terminal.

use crate::app::App;
use ratatui::{Frame, text::Line, widgets::Paragraph};

pub fn draw(frame: &mut Frame, _app: &App) {
    frame.render_widget(
        Paragraph::new(Line::from("sleepy — press q to quit")),
        frame.area(),
    );
}
