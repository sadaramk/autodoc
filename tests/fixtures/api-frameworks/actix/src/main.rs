use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

/// A stock item.
#[derive(Serialize)]
struct Item {
    sku: String,
    quantity: u32,
}

/// Body of POST /api/v1/items.
#[derive(Deserialize)]
struct NewItem {
    sku: String,
    quantity: u32,
}

/// Lists stock.
#[get("/items")]
async fn list_items() -> web::Json<Vec<Item>> {
    web::Json(vec![])
}

#[get("/items/{sku}")]
async fn get_item(path: web::Path<String>) -> impl Responder {
    HttpResponse::NotFound().finish()
}

#[post("/items")]
async fn create_item(item: web::Json<NewItem>) -> HttpResponse {
    HttpResponse::Created().json(Item { sku: item.sku.clone(), quantity: item.quantity })
}

async fn restock(path: web::Path<String>) -> HttpResponse {
    HttpResponse::Accepted().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(
            web::scope("/api").service(
                web::scope("/v1")
                    .service(list_items)
                    .service(get_item)
                    .service(create_item)
                    .route("/items/{sku}/restock", web::post().to(restock)),
            ),
        )
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
