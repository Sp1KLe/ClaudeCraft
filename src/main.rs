//! ClaudeCraft: server list screen with an edition picker (Java version / Bedrock).

use claudecraft_net::{ping_server, Edition, StatusInfo, JAVA_VERSIONS};
use macroquad::prelude::*;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

struct Server {
    name: &'static str,
    address: &'static str,
}

// Editing the list needs a text keyboard, which comes in a later step.
const SERVERS: [Server; 3] = [
    Server { name: "Hypixel", address: "mc.hypixel.net" },
    Server { name: "Local server", address: "127.0.0.1:25565" },
    Server { name: "Bedrock test", address: "127.0.0.1:19132" },
];

type PingResult = Result<StatusInfo, String>;

enum PingState {
    Idle,
    Pending(Receiver<PingResult>),
    Done(PingResult),
}

struct App {
    selected: usize,
    edition: Edition,
    picker_open: bool,
    ping: PingState,
    message: String,
}

fn edition_label(e: Edition) -> String {
    match e {
        Edition::Java(v) => format!("Java {}", v.name),
        Edition::Bedrock => "Bedrock".to_string(),
    }
}

fn button(r: Rect, label: &str, u: f32, active: bool, highlight: bool) -> bool {
    let (mx, my) = mouse_position();
    let hover = active && r.contains(vec2(mx, my));
    let bg = if highlight {
        Color::from_rgba(40, 110, 60, 255)
    } else if hover {
        Color::from_rgba(70, 90, 80, 255)
    } else {
        Color::from_rgba(50, 60, 56, 255)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, LIGHTGRAY);

    let size = (u * 0.6) as u16;
    let d = measure_text(label, None, size, 1.0);
    draw_text(
        label,
        r.x + (r.w - d.width) / 2.0,
        r.y + (r.h + d.height) / 2.0,
        size as f32,
        WHITE,
    );
    hover && is_mouse_button_pressed(MouseButton::Left)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push_str("...");
        t
    }
}

impl App {
    fn new() -> Self {
        App {
            selected: 0,
            edition: Edition::Java(JAVA_VERSIONS[0]),
            picker_open: false,
            ping: PingState::Idle,
            message: String::new(),
        }
    }

    fn update_ping(&mut self) {
        let outcome = match &self.ping {
            PingState::Pending(rx) => Some(rx.try_recv()),
            _ => None,
        };
        match outcome {
            Some(Ok(result)) => self.ping = PingState::Done(result),
            Some(Err(TryRecvError::Disconnected)) => {
                self.ping = PingState::Done(Err("ping thread stopped".to_string()))
            }
            _ => {}
        }
    }

    fn start_ping(&mut self) {
        let address = SERVERS[self.selected].address.to_string();
        let edition = self.edition;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = ping_server(&address, edition).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.ping = PingState::Pending(rx);
    }

    fn draw_info(&self, x: f32, y: f32, u: f32) {
        let size = u * 0.55;
        let line = u * 0.8;
        let mut lines: Vec<String> = Vec::new();
        match &self.ping {
            PingState::Idle => lines.push("Tap Refresh to check the server.".to_string()),
            PingState::Pending(_) => lines.push("Checking server...".to_string()),
            PingState::Done(Ok(info)) => {
                lines.push(format!(
                    "MOTD: {}",
                    truncate(&info.motd().unwrap_or_else(|| "-".to_string()), 50)
                ));
                let online = info.players_online().map(|n| n.to_string()).unwrap_or_else(|| "?".to_string());
                let max = info.players_max().map(|n| n.to_string()).unwrap_or_else(|| "?".to_string());
                lines.push(format!("Players: {} / {}", online, max));
                lines.push(format!(
                    "Server version: {}",
                    info.version_name().unwrap_or_else(|| "?".to_string())
                ));
            }
            PingState::Done(Err(e)) => lines.push(format!("Error: {}", truncate(e, 60))),
        }
        if !self.message.is_empty() {
            lines.push(self.message.clone());
        }
        for (i, l) in lines.iter().enumerate() {
            draw_text(l, x, y + line * (i as f32 + 1.0), size, LIGHTGRAY);
        }
    }

    fn draw(&mut self) {
        let u = screen_width().min(screen_height()) / 14.0;
        let m = u * 0.5;
        let w = screen_width() - 2.0 * m;
        let picker_was_open = self.picker_open;
        let active = !picker_was_open;

        draw_text("ClaudeCraft", m, u * 1.1, u * 0.9, WHITE);

        // Server list
        for (i, s) in SERVERS.iter().enumerate() {
            let r = Rect::new(m, u * 1.6 + i as f32 * u * 1.35, w, u * 1.2);
            let label = format!("{}  -  {}", s.name, s.address);
            if button(r, &label, u, active, i == self.selected) {
                self.selected = i;
                self.ping = PingState::Idle;
                self.message.clear();
            }
        }

        // Server info
        let info_y = u * 1.6 + SERVERS.len() as f32 * u * 1.35 + u * 0.1;
        self.draw_info(m, info_y, u);

        // Bottom bar
        let bar_y = screen_height() - u * 1.6;
        let bar_h = u * 1.3;
        let gap = u * 0.2;
        let edition_w = w * 0.5;
        let refresh_w = w * 0.22;
        let join_w = w - edition_w - refresh_w - 2.0 * gap;

        let edition_rect = Rect::new(m, bar_y, edition_w, bar_h);
        let refresh_rect = Rect::new(m + edition_w + gap, bar_y, refresh_w, bar_h);
        let join_rect = Rect::new(m + edition_w + refresh_w + 2.0 * gap, bar_y, join_w, bar_h);

        let edition_text = format!("{}  >", edition_label(self.edition));
        if button(edition_rect, &edition_text, u, active, false) {
            self.picker_open = true;
        }
        if button(refresh_rect, "Refresh", u, active, false) {
            self.message.clear();
            self.start_ping();
        }
        if button(join_rect, "Join", u, active, false) {
            self.message = format!(
                "Joining as {} is not implemented yet.",
                edition_label(self.edition)
            );
        }

        if picker_was_open {
            self.draw_picker(u, m, w);
        }
    }

    fn draw_picker(&mut self, u: f32, m: f32, w: f32) {
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, 0.7));

        let mut items: Vec<(String, Edition)> = JAVA_VERSIONS
            .iter()
            .map(|v| (format!("Java {}", v.name), Edition::Java(*v)))
            .collect();
        items.push(("Bedrock".to_string(), Edition::Bedrock));

        let cols = 2usize;
        let rows = (items.len() + cols - 1) / cols;
        let row_h = u * 1.1;
        let pad = u * 0.3;
        let panel_h = u * 1.4 + rows as f32 * (row_h + pad) + u * 1.5;
        let panel_y = ((screen_height() - panel_h) / 2.0).max(0.0);

        draw_rectangle(m, panel_y, w, panel_h, Color::from_rgba(30, 38, 34, 255));
        draw_rectangle_lines(m, panel_y, w, panel_h, 2.0, LIGHTGRAY);
        draw_text("Select edition", m + pad, panel_y + u * 1.0, u * 0.7, WHITE);

        let col_w = (w - pad * (cols as f32 + 1.0)) / cols as f32;
        for (idx, (label, ed)) in items.iter().enumerate() {
            let col = idx % cols;
            let row = idx / cols;
            let x = m + pad + col as f32 * (col_w + pad);
            let y = panel_y + u * 1.4 + row as f32 * (row_h + pad);
            if button(Rect::new(x, y, col_w, row_h), label, u, true, *ed == self.edition) {
                self.edition = *ed;
                self.picker_open = false;
                self.ping = PingState::Idle;
                self.message.clear();
            }
        }

        let close_y = panel_y + panel_h - u * 1.4;
        if button(Rect::new(m + w * 0.3, close_y, w * 0.4, u * 1.1), "Cancel", u, true, false) {
            self.picker_open = false;
        }
    }
}

#[macroquad::main("ClaudeCraft")]
async fn main() {
    let mut app = App::new();
    loop {
        clear_background(Color::from_rgba(24, 32, 28, 255));
        app.update_ping();
        app.draw();
        next_frame().await;
    }
}
