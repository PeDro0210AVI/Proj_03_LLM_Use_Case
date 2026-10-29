mod app;
mod ui;

use std::io;
use std::sync::Arc;

use anyhow::{Context, Result};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use app::App;
use corpo_metrics_chatbot::claude::{ApiEvent, ClaudeClient};
use corpo_metrics_chatbot::config::Config;
use corpo_metrics_chatbot::mcp::McpClient;

enum LoopEvent {
    Term(Event),
    Api(ApiEvent),
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = Arc::new(ClaudeClient::new(&config));

    let mcp = McpClient::connect(&config.mcp_server_url, config.mcp_auth_token.clone())
        .await
        .with_context(|| {
            format!(
                "couldn't reach the Corpo-Metrics MCP server at {} — start it with `cd Corpo-Metrics && cargo run`",
                config.mcp_server_url
            )
        })?;
    let mcp = Arc::new(mcp);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &config, client, mcp).await;

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
    mcp: Arc<McpClient>,
) -> Result<()> {
    let mut app = App::new(config, &mcp);

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
                handle_key(key.code, key.modifiers, &mut app, &tx, &client, &mcp);
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
    mcp: &Arc<McpClient>,
) {
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    match code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Enter => {
            if let Some(history) = app.submit_input() {
                spawn_request(history, tx.clone(), client.clone(), mcp.clone());
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
    mcp: Arc<McpClient>,
) {
    tokio::spawn(async move {
        let (api_tx, mut api_rx) = unbounded_channel::<ApiEvent>();

        tokio::spawn(async move {
            client.run_turn(history, &mcp, api_tx).await;
        });

        while let Some(event) = api_rx.recv().await {
            if tx.send(LoopEvent::Api(event)).is_err() {
                break;
            }
        }
    });
}
