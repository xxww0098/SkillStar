//! Hidden `claude-mcp-helper` entry. Claude Code starts it.
//! stdout is only MCP frames. There is no OAuth argument on this path.

use std::io::{self, Read, Write};

use skillstar_gateway::run_mcp_helper;

/// Run the helper from process argv: `skillstar claude-mcp-helper <callback> <tools>`.
pub fn run_claude_mcp_helper(args: &[String]) -> i32 {
    let rest = args.get(2..).unwrap_or(&[]);
    run_claude_mcp_helper_io(rest, io::stdin(), io::stdout(), io::stderr())
}

pub fn run_claude_mcp_helper_io(
    args: &[String],
    stdin: impl Read,
    stdout: impl Write + Send + 'static,
    stderr: impl Write,
) -> i32 {
    run_mcp_helper(args, stdin, stdout, stderr)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::run_claude_mcp_helper_io;

    #[test]
    fn helper_usage_goes_to_stderr() {
        let (mut stdout, stdout_w) = std::io::pipe().unwrap();
        let (mut stderr, stderr_w) = std::io::pipe().unwrap();
        let code = run_claude_mcp_helper_io(&[], std::io::empty(), stdout_w, stderr_w);
        assert_ne!(code, 0);
        let mut out = String::new();
        stdout.read_to_string(&mut out).unwrap();
        assert!(out.is_empty());
        let mut err = String::new();
        stderr.read_to_string(&mut err).unwrap();
        assert!(err.contains("claude MCP helper expects callback URL and tools file"));
    }
}
