#![recursion_limit = "256"]

use async_std::prelude::*;
use async_std::sync::{channel, Mutex, Receiver, Sender};
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

#[derive(Clone)]
pub struct GetLevelsHandle {
    sender: Sender<()>,
    receiver: Receiver<Vec<String>>,
}

pub async fn get_levels(handle: &mut GetLevelsHandle) -> Vec<String> {
    handle.sender.send(()).await;
    handle.receiver.next().await.unwrap()
}

pub async fn http_server(sender: Sender<File>, handle: GetLevelsHandle) -> Result<'static, ()> {
    let mut app = tide::new();
    app.at("/levels").get(move |_| {
        let mut handle = handle.clone();
        async move { get_levels(&mut handle).await.join("\n") }
    });
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
    let (levels_req_tx, mut levels_req_rx) = channel(5);
    let (levels_resp_tx, levels_resp_rx) = channel(5);
    let get_levels_handle = GetLevelsHandle {
        sender: levels_req_tx,
        receiver: levels_resp_rx,
    };

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

    task::spawn(http_server(http_tx, get_levels_handle));

    block_on(async move {
        let mut messages = Vec::new();
        loop {
            let send = async {
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
            };
            let recv = async {
                levels_req_rx.next().await.unwrap();
                let initial = messages.len() == 0;
                loop {
                    let received = channel
                        .read()
                        .messages(&http.http, |builder| {
                            if initial {
                                if let Some(oldest) = messages.last() {
                                    builder.before(oldest)
                                } else {
                                    builder
                                }
                            } else {
                                builder.after(&messages[0])
                            }
                            .limit(100)
                        })
                        .unwrap();
                    messages.extend_from_slice(&received);
                    println!("got {} messages", received.len());
                    if received.len() < 100 {
                        break;
                    }
                }
                levels_resp_tx
                    .send(
                        messages
                            .iter()
                            .filter_map(|x| x.attachments.get(0).map(|x| x.url.clone()))
                            .collect(),
                    )
                    .await;
            };
            send.race(recv).await;
        }
    });
}
