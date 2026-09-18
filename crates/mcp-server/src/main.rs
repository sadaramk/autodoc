//! `autodoc-mcp`: stdio MCP server (equivalent to `autodoc serve`).

fn main() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    autodoc_mcp::Server::default().serve(stdin.lock(), stdout.lock())
}
