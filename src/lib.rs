use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Router, routing::get};
use frankenstein::AsyncTelegramApi;
use frankenstein::client_reqwest::Bot;
use frankenstein::methods::SendMessageParams;
use frankenstein::types::ReplyParameters;
use frankenstein::updates::Update;
use frankenstein::updates::UpdateContent;
use js_sys::{Math, RegExp};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tower_service::Service;
use worker::*;

async fn root() -> &'static str {
    "Bot is running!"
}

#[derive(Deserialize)]
struct DeeplxResponse {
    // code: u16,
    // message: String,
    data: String,
    source_lang: String,
    // target_lang: String,
    // alternatives: Option<Vec<String>>,
}

#[worker::send]
async fn handle_echo(
    State(state): State<Arc<AppState>>,
    Json(update): Json<Update>,
) -> (StatusCode, &'static str) {
    let bot = Bot::new(&state.bot_token);

    match update.content {
        UpdateContent::Message(message) => {
            if let Some(text) = message.text {
                if text.contains("/version") {
                    let response = "traducteur-bot-rs version: 0.1.0".to_string();
                    let reply = SendMessageParams::builder()
                        .chat_id(message.chat.id)
                        .text(response)
                        .build();
                    match bot.send_message(&reply).await {
                        Ok(_) => {
                            console_log!("Message sent successfully.");
                        }
                        Err(e) => {
                            console_error!("Error sending message: {}", e);
                        }
                    }
                }
                if text.contains("/translate") {
                    let contains_zh = contains_zh(&text);
                    if contains_zh {
                        return (StatusCode::OK, "");
                    }
                    let _t = text.replacen("/translate", "", 1);
                    let to_be_translated = _t.trim();

                    let rn = (Math::random() * 10.0).floor() as u8;

                    let res = {
                        if rn == 0 {
                            let mut params = HashMap::new();
                            // It seems that no source_lang is all right
                            params.insert("text", to_be_translated);
                            params.insert("target_lang", "ZH");

                            Client::new()
                                .post(&state.backend_url)
                                .json(&params)
                                .send()
                                .await
                        } else {
                            Client::new()
                                .post("https://translate-pa.googleapis.com/v1/translateHtml")
                                .body(format!(
                                    "[[[\"{}\"],\"auto\",\"zh-CN\"],\"te_lib\"]",
                                    to_be_translated
                                ))
                                .send()
                                .await
                        }
                    };

                    match res {
                        Ok(response) => {
                            let r = ReplyParameters::builder()
                                .message_id(message.message_id)
                                .build();
                            if response.status().is_success() {
                                let translated_text = {
                                    if rn == 0 {
                                        let r = response.json::<DeeplxResponse>().await.unwrap();
                                        (r.data, r.source_lang)
                                    } else {
                                        let r = response.json::<Vec<Vec<String>>>().await.unwrap();
                                        let mut a = r.into_iter().flatten();
                                        (a.next().unwrap(), a.next().unwrap())
                                    }
                                };
                                let reply = SendMessageParams::builder()
                                    .chat_id(message.chat.id)
                                    .reply_parameters(r)
                                    .text(format!(
                                        "{}\n\n原文语言: {}\n\nrn = {}",
                                        translated_text.0, translated_text.1, rn
                                    ))
                                    .build();
                                match bot.send_message(&reply).await {
                                    Ok(_) => {
                                        console_log!("Message sent successfully.");
                                    }
                                    Err(e) => {
                                        console_error!("Error sending message: {}", e);
                                    }
                                }
                            } else {
                                console_error!("Error from backend: {}", response.status());
                                let reply = SendMessageParams::builder()
                                    .chat_id(message.chat.id)
                                    .reply_parameters(r)
                                    .text(format!("Error from backend: {}", response.status()))
                                    .build();
                                bot.send_message(&reply).await.unwrap();
                            }
                        }
                        Err(e) => {
                            console_error!("Error sending request to backend: {}", e);
                        }
                    }
                }
            }
            (StatusCode::OK, "")
        }
        _ => (StatusCode::OK, ""),
    }
}

struct AppState {
    bot_token: String,
    backend_url: String,
}

fn router(bot_token: String, backend_url: String) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/telegramMessage", post(handle_echo))
        .with_state(Arc::new(AppState {
            bot_token,
            backend_url,
        }))
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    _env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    console_error_panic_hook::set_once();

    let bot_token = match _env.secret("BOT_TOKEN") {
        Ok(secret) => secret.to_string(),
        Err(_) => {
            return axum::http::Response::builder()
                .status(500)
                .body("BOT_TOKEN not found".into())
                .map_err(|e| worker::Error::from(e.to_string()));
        }
    };
    let backend_url = match _env.secret("BACKEND_URL") {
        Ok(secret) => secret.to_string(),
        Err(_) => {
            return axum::http::Response::builder()
                .status(500)
                .body("BACKEND_URL not found".into())
                .map_err(|e| worker::Error::from(e.to_string()));
        }
    };
    Ok(router(bot_token, backend_url).call(req).await?)
}

fn contains_zh(text: &str) -> bool {
    let re = RegExp::new("\\p{Script_Extensions=Han}", "v");
    re.test(text)
}
