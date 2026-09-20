//! `nunki-mcp`: stdio MCP server (equivalent to `nunki serve`).

fn main() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    nunki_mcp::Server::default().serve(stdin.lock(), stdout.lock())
}
