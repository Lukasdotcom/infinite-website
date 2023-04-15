use actix_web::*;
use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[get("/{route:.*}")]
async fn website(path: web::Path<String>) -> impl Responder {
    // Tries to determine the content type
    let b = path.find("?").unwrap_or(path.len());
    let reversed_path = &path[..b].chars().rev().collect::<String>();
    let a = (path.len() as i32 - reversed_path.find(".").unwrap_or(0) as i32) as usize;
    let content_type = match &path[a..] {
        "js" => "application/javascript",
        "css" => "text/css",
        "ico" => "image/svg",
        "png" => "image/svg",
        "jpg" => "image/svg",
        "svg" => "image/svg",
        _ => "text/html",
    };
    println!("Creating document for {}", path);
    // Creates the chat
    let mut headers = header::HeaderMap::new();
    let cookies = format!(
        "__Secure-next-auth.session-token={};",
        std::env::var("COOKIE").unwrap()
    );
    headers.insert(header::COOKIE, cookies.parse().unwrap());
    #[derive(Serialize, Deserialize)]
    struct Response {
        pub id: String,
        pub created_at: String,
        pub modified_at: String,
        pub title: Value,
        pub hidden: bool,
    }
    let client = reqwest::Client::new();
    let res = client
        .post("https://open-assistant.io/api/chat")
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json::<Response>()
        .await
        .unwrap();
    let id = res.id;
    // Sends the message
    let mut headers = header::HeaderMap::new();
    headers.insert(header::COOKIE, cookies.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    #[derive(Serialize, Deserialize)]
    struct Response2 {
        pub id: String,
        pub parent_id: Value,
        pub content: String,
        pub chat_id: String,
        pub created_at: String,
        pub role: String,
        pub state: String,
        pub score: i64,
        pub reports: Vec<Value>,
        pub work_parameters: Value,
    }
    let client = reqwest::Client::new();
    #[derive(Serialize, Deserialize)]
    struct Body {
        chat_id: String,
        content: String,
        parent_id: Value,
    }
    let body_text = serde_json::to_string(&Body {
        chat_id: id,
        content: match content_type {
            "application/javascript" => format!("Create a javascript file with content that matches the following URL path:
`/{}`", path),
            "text/css" => format!("Create a css file with content that matches the following URL path:
`/{}`", path),
            "image/svg" => format!("Create a svg file with content that matches the following URL path:
`/{}`", path),
            _ => format!("Create a HTML response document with content that matches the following URL path:
`/{}`
Add href links on the same site with related topics.", path),
        },
        parent_id: Value::Null,
    });
    let body_text = body_text.unwrap();
    let res = client
        .post("https://open-assistant.io/api/chat/prompter_message")
        .headers(headers)
        .body(body_text)
        .send()
        .await
        .unwrap()
        .json::<Response2>()
        .await
        .unwrap();
    let parent_id = res.id;
    let chat_id = res.chat_id;
    // Sends the model type
    let mut headers = header::HeaderMap::new();
    headers.insert(header::COOKIE, cookies.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    #[derive(Serialize, Deserialize)]
    struct Response3 {
        pub id: String,
        pub parent_id: String,
        pub content: Value,
        pub chat_id: String,
        pub created_at: String,
        pub role: String,
        pub state: String,
        pub score: i64,
        pub reports: Vec<Value>,
        pub work_parameters: WorkParameters,
    }
    #[derive(Serialize, Deserialize)]
    pub struct WorkParameters {
        pub model_config: ModelConfig,
        pub sampling_parameters: SamplingParameters,
        pub do_sample: bool,
        pub seed: f64,
    }
    #[derive(Serialize, Deserialize)]
    pub struct ModelConfig {
        pub model_id: String,
        pub max_input_length: i64,
        pub max_total_length: i64,
        pub quantized: bool,
    }
    #[derive(Serialize, Deserialize)]
    pub struct SamplingParameters {
        pub top_k: f64,
        pub top_p: f64,
        pub typical_p: Value,
        pub temperature: f64,
        pub repetition_penalty: f64,
        pub max_new_tokens: i64,
    }
    let client = reqwest::Client::new();
    let res = client.post("https://open-assistant.io/api/chat/assistant_message")
        .headers(headers)
        .body(format!("{{\"chat_id\":\"{}\",\"parent_id\":\"{}\",\"model_config_name\":\"OA_SFT_Llama_30B_6\",\"sampling_parameters\":{{\"top_k\":50,\"top_p\":0.95,\"typical_p\":null,\"temperature\":1,\"repetition_penalty\":1.2,\"max_new_tokens\":1024}}}}", chat_id, parent_id))
        .send()
        .await
        .unwrap()
        .json::<Response3>()
        .await
        .unwrap();
    let message_id = res.id;
    let chat_id = res.chat_id;
    // Gets the AI response
    let mut headers = header::HeaderMap::new();
    headers.insert(header::COOKIE, cookies.parse().unwrap());
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "https://open-assistant.io/api/chat/events?chat_id={}&message_id={}",
            chat_id, message_id
        ))
        .headers(headers)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    // Reads the response and responds with the result
    #[derive(Serialize, Deserialize)]
    pub struct Root {
        pub event_type: String,
        pub message: Message,
    }
    #[derive(Serialize, Deserialize)]
    pub struct Message {
        pub id: String,
        pub parent_id: String,
        pub content: String,
        pub chat_id: String,
        pub created_at: String,
        pub role: String,
        pub state: String,
        pub score: i64,
        pub reports: Vec<Value>,
        pub work_parameters: WorkParameters,
    }
    let res = res
        .lines()
        .filter(|x| x.contains("\"event_type\": \"message\""))
        .last()
        .expect("Failed to get response");
    let res = serde_json::from_str::<Root>(&res[6..]).unwrap();
    let res = res.message.content;
    let start_bytes = match res.find("```") {
        Some(x) => match &res[x + 3..].find("\n") {
            Some(y) => x + 3 + y + 1,
            None => x + 3,
        },
        None => 0,
    };
    let end_bytes = match res.chars().rev().collect::<String>().find("```") {
        Some(x) => res.len() - x - 3,
        None => res.len(),
    };
    HttpResponse::Ok().content_type(content_type).body(format!("{}", &res[start_bytes..end_bytes]))
}
#[actix_web::main]
pub async fn main() -> std::io::Result<()> {
    std::fs::create_dir("./db").unwrap_or(());
    println!("Starting server...");
    HttpServer::new(|| App::new().service(website))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
