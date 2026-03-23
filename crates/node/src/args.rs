use clap::Args;
use std::net::SocketAddr;

const DEFAULT_REMOTE_EXEX_BUFFER: usize = 1024;

fn parse_remote_exex_buffer(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|err| format!("invalid buffer size: {err}"))?;
    if parsed == 0 {
        return Err("remote ExEx buffer must be greater than zero".to_string());
    }
    Ok(parsed)
}

/// Evolve CLI arguments (currently empty; reserved for future toggles).
#[derive(Debug, Clone, Default, Args)]
pub struct EvolveArgs {
    /// Listen address for the built-in Remote `ExEx` gRPC server.
    #[arg(long, value_name = "SOCKET_ADDR")]
    pub remote_exex_grpc_listen_addr: Option<SocketAddr>,
    /// Bounded notification buffer shared by Remote `ExEx` subscribers.
    #[arg(
        long,
        value_name = "N",
        default_value_t = DEFAULT_REMOTE_EXEX_BUFFER,
        value_parser = parse_remote_exex_buffer
    )]
    pub remote_exex_buffer: usize,
}

#[cfg(test)]
mod tests {
    use super::EvolveArgs;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(flatten)]
        args: EvolveArgs,
    }

    #[test]
    fn remote_exex_defaults_to_disabled() {
        let cli = TestCli::try_parse_from(["test"]).expect("default cli should parse");
        assert!(cli.args.remote_exex_grpc_listen_addr.is_none());
        assert_eq!(cli.args.remote_exex_buffer, 1024);
    }

    #[test]
    fn remote_exex_flags_parse() {
        let cli = TestCli::try_parse_from([
            "test",
            "--remote-exex-grpc-listen-addr",
            "127.0.0.1:30001",
            "--remote-exex-buffer",
            "16",
        ])
        .expect("remote exex cli should parse");

        assert_eq!(
            cli.args
                .remote_exex_grpc_listen_addr
                .expect("listen address"),
            "127.0.0.1:30001".parse().unwrap()
        );
        assert_eq!(cli.args.remote_exex_buffer, 16);
    }
}
