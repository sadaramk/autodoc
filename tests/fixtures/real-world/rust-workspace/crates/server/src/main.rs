//! Forum server binary.

use actix_web::{App, HttpServer};
use db_views_a::list_a;
use db_views_b::list_b;
use db_views_c::list_c;
use db_views_d::list_d;
use db_views_e::list_e;
use db_views_f::list_f;

/// Starts the HTTP server.
fn main() -> std::io::Result<()> {
    ws_utils::init_logging();
    let _ = list_a();
    let _ = list_b();
    let _ = list_c();
    let _ = list_d();
    let _ = list_e();
    let _ = list_f();
    let _server = HttpServer::new(|| App::new());
    Ok(())
}
