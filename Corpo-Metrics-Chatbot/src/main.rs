mod app;
mod claude;
mod config;
mod local_mcp;
mod ui;

use std::io;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use app::App;
use claude::{ApiEvent, ClaudeClient};
use config::Config;
use local_mcp::LocalMcpRegistry;

enum LoopEvent {
    Term(Event),
    Api(ApiEvent),
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = Arc::new(ClaudeClient::new(&config));

    let local = Arc::new(LocalMcpRegistry::connect(config.local_mcp_servers()).await);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &config, client, local).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &Config,
    client: Arc<ClaudeClient>,
    local: Arc<LocalMcpRegistry>,
) -> Result<()> {
    let mut app = App::new(config, &local);

    let (tx, mut rx) = unbounded_channel::<LoopEvent>();

    {
        let tx = tx.clone();
        std::thread::spawn(move || loop {
            match event::read() {
                Ok(ev) => {
                    if tx.send(LoopEvent::Term(ev)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        });
    }

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let Some(loop_event) = rx.recv().await else {
            break;
        };

        match loop_event {
            LoopEvent::Term(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                handle_key(key.code, key.modifiers, &mut app, &tx, &client, &local);
            }
            LoopEvent::Term(_) => {}
            LoopEvent::Api(event) => app.handle_api_event(event),
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_key(
    code: KeyCode,
    modifiers: KeyModifiers,
    app: &mut App,
    tx: &UnboundedSender<LoopEvent>,
    client: &Arc<ClaudeClient>,
    local: &Arc<LocalMcpRegistry>,
) {
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    match code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Enter => {
            if let Some(history) = app.submit_input() {
                spawn_request(history, tx.clone(), client.clone(), local.clone());
            }
        }
        KeyCode::Backspace => {
            if !app.is_streaming {
                app.input.pop();
            }
        }
        KeyCode::Char(c) => {
            if !app.is_streaming {
                app.input.push(c);
            }
        }
        KeyCode::Up => app.scroll_up(1),
        KeyCode::Down => app.scroll_down(1),
        KeyCode::PageUp => app.scroll_up(10),
        KeyCode::PageDown => app.scroll_down(10),
        KeyCode::End => app.jump_to_bottom(),
        _ => {}
    }
}

/// Fires off a Claude API call on its own task and relays every streamed
/// event back into the main loop's channel as it arrives.
fn spawn_request(
    history: Vec<serde_json::Value>,
    tx: UnboundedSender<LoopEvent>,
    client: Arc<ClaudeClient>,
    local: Arc<LocalMcpRegistry>,
) {
    tokio::spawn(async move {
        let (api_tx, mut api_rx) = unbounded_channel::<ApiEvent>();

        tokio::spawn(async move {
            client.run_turn(history, &local, api_tx).await;
        });

        while let Some(event) = api_rx.recv().await {
            if tx.send(LoopEvent::Api(event)).is_err() {
                break;
            }
        }
    });
}
