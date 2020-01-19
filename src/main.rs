#![recursion_limit = "256"]

use async_std::prelude::*;
use async_std::sync::{channel, Mutex, Sender};
use async_std::task::{self, block_on};
use http::status::StatusCode;
use serenity::http::AttachmentType;
use serenity::prelude::*;
use std::{iter, thread};
use tide::{IntoResponse, Request};

type Result<'a, T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'a>>;

pub struct File {
    pub data: Vec<u8>,
    pub name: String,
}

pub async fn http_server(sender: Sender<File>) -> Result<'static, ()> {
    let mut app = tide::new();
    app.at("/upload").post(move |req: Request<()>| {
        let sender = sender.clone();
        let req = Mutex::new(req);
        async move {
            let filename = req
                .lock()
                .await
                .header("X-VVVVVV-Filename")
                .map(<_>::to_string);
            if let Some(name) = filename {
                let mut data = Vec::with_capacity(
                    req.lock()
                        .await
                        .header("Content-Length")
                        .and_then(|len| {
                            if let Ok(len) = len.parse() {
                                if len <= 1024 * 1024 {
                                    Some(len)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0),
                );
                let is_ok = req
                    .lock()
                    .await
                    .by_ref()
                    .take(1024 * 1024)
                    .read_to_end(&mut data)
                    .await
                    .is_ok();
                if is_ok {
                    if req.lock().await.by_ref().bytes().next().await.is_none() {
                        sender.send(File { name, data }).await;
                        return "Data received!".into_response();
                    } else {
                        return "Data too long!"
                            .with_status(StatusCode::BAD_REQUEST)
                            .into_response();
                    }
                }
            }
            "Missing data!"
                .with_status(StatusCode::BAD_REQUEST)
                .into_response()
        }
    });
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}

#[allow(clippy::unreadable_literal)]
const CHANNEL_ID: u64 = 668575952900718595;

struct NullHandler;

impl EventHandler for NullHandler {}

fn main() {
    let (http_tx, mut http_rx) = channel(5);

    let token = dotenv::var("DISCORD_TOKEN").unwrap();
    let mut client = Client::new(&token, NullHandler).unwrap();

    let channel = client
        .cache_and_http
        .http
        .get_channel(CHANNEL_ID)
        .unwrap()
        .guild()
        .unwrap();
    let http = client.cache_and_http.clone();

    thread::spawn(move || client.start().unwrap());

    task::spawn(http_server(http_tx));

    block_on(async move {
        loop {
            let file = http_rx.next().await.unwrap();
            channel
                .read()
                .send_files(
                    &http.http,
                    iter::once(AttachmentType::Bytes {
                        data: file.data.into(),
                        filename: file.name,
                    }),
                    |msg| msg.content("New level!"),
                )
                .unwrap();
        }
    });
}
