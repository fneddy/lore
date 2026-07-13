# Lore

A fast documentation search tool with MCP (Model Context Protocol) server support for querying project and system documentation.

## What is Lore?

Lore provides efficient full-text search across:
- **Project documentation**: Rust docs (rustdoc), Doxygen output, and local docs
- **System documentation**: Man pages, `/usr/share/doc`, and system-wide libraries

It includes multiple CLI tools and an MCP server for integration with AI assistants.

## Installation

Install directly from GitHub using cargo:

```bash
cargo install --git https://github.com/fneddy/lore.git lore-cli
```

This will install the following binaries:
- `lore` - Main CLI tool
- `system-lore` - System documentation search
- `project-lore` - Project documentation search
- `mcp-lore` - MCP server for AI assistant integration

## MCP Server Configuration

To use Lore with AI assistants that support MCP (like Claude Desktop), add the following to your MCP configuration file:

### Claude Desktop Configuration

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows) or `~/.config/Claude/claude_desktop_config.json` (Linux):

```json
{
  "mcpServers": {
    "lore": {
      "command": "mcp-lore",
      "args": []
    }
  }
}
```

## Usage

### CLI Usage

Search project documentation:
```bash
project-lore "async" "tokio"
```

Search system documentation:
```bash
system-lore "socket" "TCP"
```

### MCP Server

Once configured, the MCP server provides tools for AI assistants to:
- Query project documentation
- Query system documentation
- List available modules
- Show documentation content
- Extract documentation outlines

The AI assistant can use these tools to help you understand APIs, find documentation, and answer technical questions about your codebase and system libraries.

## License

BSD 3-Clause
