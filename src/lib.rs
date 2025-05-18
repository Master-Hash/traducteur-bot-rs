use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Router, routing::get};
use frankenstein::AsyncTelegramApi;
use frankenstein::client_reqwest::Bot;
use frankenstein::methods::SendMessageParams;
use frankenstein::updates::Update;
use frankenstein::updates::UpdateContent;
use js_sys::RegExp;
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
    // source_lang: String,
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
                if text == "/version" {
                    let response = "tgbot-worker-rs version: 0.1.0".to_string();
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
                if text.starts_with("/translate") {
                    let contains_zh = contains_zh(&text);
                    if contains_zh {
                        return (StatusCode::OK, "");
                    }
                    let _t = text.replacen("/translate", "", 1);
                    let to_be_translated = _t.trim();
                    let mut params = HashMap::new();
                    // It seems that no source_lang is all right
                    params.insert("text", to_be_translated);
                    params.insert("target_lang", "ZH");

                    let res = Client::new()
                        .post(&state.backend_url)
                        .json(&params)
                        .send()
                        .await;

                    match res {
                        Ok(response) => {
                            if response.status().is_success() {
                                let translated_text =
                                    response.json::<DeeplxResponse>().await.unwrap();
                                let reply = SendMessageParams::builder()
                                    .chat_id(message.chat.id)
                                    .text(translated_text.data)
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
