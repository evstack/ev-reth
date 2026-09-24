use clap::Args;
use url::Url;

/// Evolve CLI arguments.
#[derive(Debug, Clone, Default, Args)]
pub struct EvolveArgs {
    /// Subscribe to valid forkchoice updates from an authoritative ev-reth peer.
    ///
    /// The subscriber receives only chain identity and forkchoice references from this endpoint.
    /// Block headers and bodies are fetched through the configured native Reth P2P peers.
    #[arg(
        long,
        value_name = "WS_URL",
        env = "EV_SUBSCRIBE_PEER",
        value_parser = parse_websocket_url
    )]
    pub subscribe_peer: Option<Url>,
}

fn parse_websocket_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|error| error.to_string())?;
    if matches!(url.scheme(), "ws" | "wss") {
        Ok(url)
    } else {
        Err("peer endpoint must use ws:// or wss://".into())
    }
}

#[cfg(test)]
mod tests {
    use super::EvolveArgs;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        evolve: EvolveArgs,
    }

    #[test]
    fn parses_peer_websocket_url() {
        let cli =
            TestCli::try_parse_from(["ev-reth", "--subscribe-peer", "wss://peer.example/rpc"])
                .expect("valid peer subscription argument");

        assert_eq!(
            cli.evolve.subscribe_peer.expect("configured URL").as_str(),
            "wss://peer.example/rpc"
        );
    }

    #[test]
    fn rejects_non_websocket_peer_url() {
        assert!(TestCli::try_parse_from([
            "ev-reth",
            "--subscribe-peer",
            "https://peer.example/rpc",
        ])
        .is_err());
    }
}
