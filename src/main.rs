use actix_web::{
    http::header::{CacheControl, CacheDirective},
    *,
};
use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};
use sqlx::{migrate::MigrateDatabase, Connection};
use std::env;
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
        let res = match sqlx::query_as::<_, Cache>("SELECT * FROM cache WHERE path=?")
            .bind(&path)
            .fetch_one(&mut conn)
            .await
        {
            Ok(cache) => {
                println!("Getting cached response for {}", path);
                Ok(cache.content)
            }
            Err(_) => {
                println!("Creating document for {}", path);
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
                let res = ollama.generate(GenerationRequest::new(model, prompt)).await;
                match res {
                    Ok(res) => {
                        let res = res.response;
                        let start_bytes = match res.find("```") {
                            Some(x) => match &res[x + 3..].find('\n') {
                                Some(y) => x + 3 + y + 1,
                                None => x + 3,
                            },
                            None => 0,
                        };
                        let mut end_bytes = match res.chars().rev().collect::<String>().find("```")
                        {
                            Some(x) => res.len() - x - 3,
                            None => res.len(),
                        };
                        if start_bytes > end_bytes {
                            end_bytes = res.len();
                        }
                        let res = match start_bytes > end_bytes {
                            true => res,
                            false => (res[start_bytes..end_bytes]).to_string(),
                        };
                        sqlx::query("INSERT INTO cache (path, content) VALUES (?, ?)")
                            .bind(&path)
                            .bind(res.clone())
                            .execute(&mut conn)
                            .await
                            .unwrap();
                        Ok(res)
                    }
                    Err(x) => {
                        println!("Ollama failed with error: {}, for page {}", x, path);
                        Err(x)
                    }
                }
            }
        };
        match res {
            Ok(res) => {
                let res = if content_type == "text/html" {
                    res + &format!(
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
                    res.to_string()
                };
                HttpResponse::Ok()
                    .insert_header(CacheControl(vec![
                        CacheDirective::Public,
                        CacheDirective::MaxAge(604800),
                    ]))
                    .content_type(content_type)
                    .body(res)
            }
            Err(_) => HttpResponse::Ok()
                .content_type("text/plain")
                .body("Error generating page"),
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
    println!("Starting server...");
    HttpServer::new(|| App::new().service(website))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
