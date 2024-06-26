use actix_web::{
    http::header::{CacheControl, CacheDirective},
    web::{BufMut, Bytes, BytesMut},
    *,
};
use broadcaster::BroadcastChannel;
use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};
use sqlx::{migrate::MigrateDatabase, Connection, SqliteConnection};
use std::iter::Iterator;
use std::{env, thread};
use tokio_stream::StreamExt;
lazy_static::lazy_static! {
pub static ref CACHE_EVENTS: BroadcastChannel<(String, String)> = BroadcastChannel::new();
}
async fn cache(conn: &mut SqliteConnection, content: &str, path: &str) {
    let result = sqlx::query("INSERT INTO cache (path, content) VALUES (?, ?)")
        .bind(path)
        .bind(content)
        .execute(conn)
        .await;
    if result.is_err() {
        println!("Failed to cache `{}`", path);
    }
}
#[routes]
#[get("/{route:.*}")]
#[post("/{route:.*}")]
async fn website(path: web::Path<String>, req: HttpRequest) -> impl Responder {
    // Tries to determine the content type
    let reversed_path = path.chars().rev().collect::<String>();
    let a = (path.len() as i32 - reversed_path.find('.').unwrap_or(0) as i32) as usize;
    let content_type = match &path[a..] {
        "js" => "application/javascript",
        "json" => "application/json",
        "css" => "text/css",
        "ico" => "image/jpeg",
        "png" => "image/jpeg",
        "jpg" => "image/jpeg",
        "jpeg" => "image/jpeg",
        "gif" => "image/jpeg",
        "svg" => "image/jpeg",
        "xml" => "application/xml",
        _ => "text/html",
    };
    let css = if req.query_string().is_empty() {
        path.to_string() + ".css"
    } else {
        path.to_string() + ".css?" + req.query_string()
    };
    let path = if req.query_string().is_empty() {
        path.to_string()
    } else {
        path.to_string() + "?" + req.query_string()
    };
    // Checks if this was an image
    if content_type == "image/jpeg" {
        println!("Getting image for {}", path);
        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "https://bing.com/images/search?q={}",
                urlencoding::encode(&path)
            ))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let a = res.find("class=\"mimg\"").unwrap_or(0);
        let res = &res[a..];
        let a = res.find("src=\"").unwrap_or(0) + 5;
        let res = &res[a..];
        let b = res.find('"').unwrap_or(0);
        let res = &res[..b];
        let client = reqwest::Client::new();
        let res = client.get(res).send().await.unwrap().bytes().await.unwrap();
        HttpResponse::Ok()
            .insert_header(CacheControl(vec![
                CacheDirective::Public,
                CacheDirective::MaxAge(604800),
            ]))
            .content_type("image/jpeg")
            .body(res)
    } else {
        let mut conn = sqlx::SqliteConnection::connect("sqlite:./cache/db.db")
            .await
            .unwrap();
        #[derive(sqlx::FromRow)]
        struct Cache {
            #[allow(dead_code)]
            pub path: String,
            pub content: String,
        }
        let extra = if content_type == "text/html" {
            format!(
                "
<!-- The following code was not generated by the AI, but is for analytics. -->
<script>
var _paq = window._paq = window._paq || [];
_paq.push(['trackPageView']);
_paq.push(['enableLinkTracking']);
(function() {{
    var u=`//analytics.lschaefer.xyz/`;
    _paq.push(['setTrackerUrl', u+'matomo.php']);
    _paq.push(['setSiteId', '1']);
    var d=document, g=d.createElement('script'), s=d.getElementsByTagName('script')[0];
    g.async=true; g.src=u+'matomo.js'; s.parentNode.insertBefore(g,s);
}})();
</script>
<link rel=\"stylesheet\" type=\"text/css\" href=\"/{}\" />",
                css
            )
        } else {
            "".to_owned()
        };
        match sqlx::query_as::<_, Cache>("SELECT * FROM cache WHERE path=?")
            .bind(&path)
            .fetch_one(&mut conn)
            .await
        {
            Ok(cache) => {
                println!("Serving cached response for: `{}`", path);
                HttpResponse::Ok()
                    .insert_header(CacheControl(vec![
                        CacheDirective::Public,
                        CacheDirective::MaxAge(604800),
                    ]))
                    .content_type(content_type)
                    .body(
                        if content_type == "text/html" {
                            cache.content
                        } else {
                            "".to_string()
                        } + &extra,
                    )
            }
            Err(_) => {
                println!("Creating document for: `{}`", path);
                // Creates the prompt
                let prompt = match content_type {
                    "application/javascript" => format!(
                        "Create a vanilla javascript file that would solve the goal for the following url path:
`/{}`",
                        path
                    ),
                    "text/css" => format!(
                        "Create a css file with modern styles that matches the following URL path:
`/{}`",
                        path
                    ),
                    "application/json" => format!(
                        "Create a json file with content that matches the following URL path:
`/{}`",
                        path
                    ),
                    "application/xml" => format!(
                        "Create a xml file with content that matches the following URL path:
`/{}`",
                        path
                    ),
                    _ =>
                    format!(
                        "Create a HTML response document with content that matches the following URL path:
`/{}`
Add href links on the same site with related topics.",
                        path)
                };
                let model = env::var("MODEL").unwrap_or("llama3:latest".to_string());
                let host = env::var("OLLAMA_HOST");
                let port = env::var("OLLAMA_PORT")
                    .unwrap_or("2".to_string())
                    .parse::<u16>();
                let ollama = match (host, port) {
                    (Ok(x), Ok(y)) => Ollama::new(x, y),
                    _ => Ollama::default(),
                };
                let stream = ollama
                    .generate_stream(GenerationRequest::new(model, prompt))
                    .await
                    .unwrap();
                let mut stage = 0;
                let mut content = String::new();
                let stream = StreamExt::map_while(stream, move |res| {
                    if stage >= 7 {
                        println!("Finished generating document for: `{}`", path);
                        let content2 = content.clone();
                        let path2 = path.clone();
                        // Sends an event that adds to the cache
                        thread::spawn(move || {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .unwrap();
                            let _ =
                                rt.block_on(async { CACHE_EVENTS.send(&(content2, path2)).await });
                        });

                        return None;
                    }
                    let mut response = BytesMut::new();
                    for response_part in res.unwrap() {
                        let part = response_part.response;
                        for char in part.chars() {
                            // Stage 0 is before the first code block, stage 1, 2, and 3 are the starting code block. Stage 4 the new line, stage 5, 6, and 7 are the closing code block
                            if char == '`' {
                                if stage != 3 {
                                    stage += 1;
                                }
                            } else if stage == 1 || stage == 2 {
                                stage = 0;
                            } else if stage == 6 {
                                response.put_u8(b'`');
                                content.push('`');
                                response.put_u8(b'`');
                                content.push('`');
                                stage -= 2;
                            } else if stage == 5 {
                                response.put_u8(b'`');
                                content.push('`');
                                stage -= 1;
                            } else if stage == 3 && char == '\n' {
                                stage += 1;
                            }
                            if stage == 7 {
                                if content_type == "text/html" {
                                    response.put(extra.as_bytes());
                                }
                                break;
                            }
                            if stage == 4 {
                                response.put_u8(char as u8);
                                content.push(char);
                            }
                        }
                    }
                    Some(Ok::<_, actix_web::Error>(Bytes::from(response)))
                });
                HttpResponse::Ok()
                    .insert_header(CacheControl(vec![
                        CacheDirective::Public,
                        CacheDirective::MaxAge(604800),
                    ]))
                    .streaming(stream)
            }
        }
    }
}
#[actix_web::main]
pub async fn main() -> std::io::Result<()> {
    std::fs::create_dir("./cache").unwrap_or(());
    let _ = sqlx::Sqlite::create_database("./cache/db.db").await;
    let mut conn = sqlx::SqliteConnection::connect("sqlite:./cache/db.db")
        .await
        .unwrap();
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS cache (
            path TEXT PRIMARY KEY NOT NULL,
            content TEXT NOT NULL
        )",
    )
    .execute(&mut conn)
    .await;
    // Recieves all cache events and stores them
    let mut events = CACHE_EVENTS.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let (content, path) = event;
            cache(&mut conn, &content, &path).await;
        }
    });
    println!("Starting server...");
    HttpServer::new(|| App::new().service(website))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
