use std::{collections::VecDeque, time::Instant};

use alloy_primitives::{Address, U256};
use tokio::sync::mpsc;

const MAX_LOGS: usize = 1000;
const MAX_BLOCKS: usize = 200;

#[derive(Debug, Clone)]
pub(crate) struct BlockInfo {
    pub(crate) number: u64,
    pub(crate) hash: String,
    pub(crate) tx_count: u64,
    pub(crate) gas_used: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct LogEntry {
    pub(crate) level: tracing::Level,
    pub(crate) target: String,
    pub(crate) message: String,
    pub(crate) fields: Vec<(String, String)>,
    pub(crate) timestamp: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct TxInfo {
    pub(crate) hash: String,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BlockDetail {
    pub(crate) number: u64,
    pub(crate) txs: Vec<TxInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Panel {
    Blocks,
    Logs,
    Accounts,
}

pub(crate) struct App {
    // Static
    pub(crate) chain_id: u64,
    pub(crate) rpc_url: String,
    pub(crate) block_time: u64,
    pub(crate) accounts: Vec<(String, String)>,
    pub(crate) deploy_contracts: Option<Vec<(String, String)>>,

    // Dynamic
    pub(crate) blocks: VecDeque<BlockInfo>,
    pub(crate) logs: VecDeque<LogEntry>,
    pub(crate) current_block: u64,
    pub(crate) start_time: Instant,
    pub(crate) balances: Vec<String>,

    // UI state
    pub(crate) active_panel: Panel,
    pub(crate) log_scroll: usize,
    pub(crate) block_selected: usize,
    pub(crate) account_selected: usize,
    pub(crate) clipboard_msg: Option<(String, Instant)>,
    pub(crate) block_detail: Option<BlockDetail>,
    pub(crate) should_quit: bool,

    // Channels
    pub(crate) log_rx: mpsc::Receiver<LogEntry>,
    pub(crate) balance_rx: mpsc::Receiver<Vec<String>>,
    pub(crate) detail_tx: mpsc::Sender<BlockDetail>,
    pub(crate) detail_rx: mpsc::Receiver<BlockDetail>,
}

impl App {
    pub(crate) fn new(
        chain_id: u64,
        rpc_url: String,
        block_time: u64,
        accounts: Vec<(String, String)>,
        deploy_contracts: Option<Vec<(String, String)>>,
        log_rx: mpsc::Receiver<LogEntry>,
        balance_rx: mpsc::Receiver<Vec<String>>,
    ) -> Self {
        let initial_balance = "1000000 ETH".to_string();
        let balances = vec![initial_balance; accounts.len()];
        let (detail_tx, detail_rx) = mpsc::channel(4);
        Self {
            chain_id,
            rpc_url,
            block_time,
            accounts,
            deploy_contracts,
            blocks: VecDeque::new(),
            logs: VecDeque::new(),
            current_block: 0,
            start_time: Instant::now(),
            balances,
            active_panel: Panel::Logs,
            log_scroll: 0,
            block_selected: 0,
            account_selected: 0,
            clipboard_msg: None,
            block_detail: None,
            should_quit: false,
            log_rx,
            balance_rx,
            detail_tx,
            detail_rx,
        }
    }

    pub(crate) fn drain_balances(&mut self) {
        while let Ok(new_balances) = self.balance_rx.try_recv() {
            self.balances = new_balances;
        }
    }

    pub(crate) fn drain_logs(&mut self) {
        while let Ok(entry) = self.log_rx.try_recv() {
            if entry.message == "built block" {
                if let Some(block) = self.parse_block_from_fields(&entry.fields) {
                    self.current_block = block.number;
                    self.blocks.push_front(block);
                    if self.blocks.len() > MAX_BLOCKS {
                        self.blocks.pop_back();
                    }
                }
            }

            self.logs.push_back(entry);
            if self.logs.len() > MAX_LOGS {
                self.logs.pop_front();
            }
        }
    }

    fn parse_block_from_fields(&self, fields: &[(String, String)]) -> Option<BlockInfo> {
        let mut number = None;
        let mut hash = String::new();
        let mut tx_count = 0;
        let mut gas_used = 0;

        for (k, v) in fields {
            match k.as_str() {
                "block_number" => number = v.parse().ok(),
                "block_hash" => {
                    let h = v.trim_matches('"');
                    hash = if h.len() > 10 {
                        format!("{}..{}", &h[..6], &h[h.len() - 4..])
                    } else {
                        h.to_string()
                    };
                }
                "tx_count" => tx_count = v.parse().unwrap_or(0),
                "gas_used" => gas_used = v.parse().unwrap_or(0),
                _ => {}
            }
        }

        number.map(|n| BlockInfo {
            number: n,
            hash,
            tx_count,
            gas_used,
        })
    }

    pub(crate) fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Blocks => Panel::Logs,
            Panel::Logs => Panel::Accounts,
            Panel::Accounts => Panel::Blocks,
        };
    }

    pub(crate) fn scroll_up(&mut self) {
        match self.active_panel {
            Panel::Logs => self.log_scroll = self.log_scroll.saturating_add(1),
            Panel::Blocks => {
                self.block_selected = self.block_selected.saturating_sub(1);
            }
            Panel::Accounts => {
                self.account_selected = self.account_selected.saturating_sub(1);
            }
        }
    }

    pub(crate) fn scroll_down(&mut self) {
        match self.active_panel {
            Panel::Logs => self.log_scroll = self.log_scroll.saturating_sub(1),
            Panel::Blocks => {
                if !self.blocks.is_empty() {
                    self.block_selected = (self.block_selected + 1).min(self.blocks.len() - 1);
                }
            }
            Panel::Accounts => {
                if !self.accounts.is_empty() {
                    self.account_selected =
                        (self.account_selected + 1).min(self.accounts.len() - 1);
                }
            }
        }
    }

    pub(crate) fn copy_account_address(&mut self) {
        if let Some((addr, _)) = self.accounts.get(self.account_selected) {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(addr.clone());
                let truncated = if addr.len() > 10 {
                    format!("{}..{}", &addr[..6], &addr[addr.len() - 4..])
                } else {
                    addr.clone()
                };
                self.clipboard_msg = Some((format!("Copied address {truncated}"), Instant::now()));
            }
        }
    }

    pub(crate) fn copy_account_key(&mut self) {
        if let Some((_, key)) = self.accounts.get(self.account_selected) {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(key.clone());
                self.clipboard_msg = Some(("Copied private key".to_string(), Instant::now()));
            }
        }
    }

    pub(crate) fn fetch_block_detail(&self) {
        let Some(block_info) = self.blocks.get(self.block_selected) else {
            return;
        };
        let tx = self.detail_tx.clone();
        let rpc_url = self.rpc_url.clone();
        let block_num = block_info.number;

        tokio::spawn(async move {
            use alloy_network::TransactionResponse;
            use alloy_provider::{Provider, ProviderBuilder};
            use alloy_rpc_types::{BlockNumberOrTag, TransactionTrait};

            let provider =
                ProviderBuilder::new().connect_http(rpc_url.parse().expect("valid RPC URL"));

            let result = provider
                .get_block_by_number(BlockNumberOrTag::Number(block_num))
                .full()
                .await;

            let txs = match result {
                Ok(Some(block)) => block
                    .transactions
                    .into_transactions()
                    .map(|t| {
                        let hash = format!("{}", t.tx_hash());
                        let from = format!("{}", t.from());
                        let to = t.to().map_or("Contract Creation".into(), |a| {
                            truncate_hex(&format!("{a}"))
                        });
                        let value = format_ether(t.value());
                        TxInfo {
                            hash: truncate_hex(&hash),
                            from: truncate_hex(&from),
                            to,
                            value,
                        }
                    })
                    .collect(),
                _ => vec![],
            };

            let _ = tx
                .send(BlockDetail {
                    number: block_num,
                    txs,
                })
                .await;
        });
    }

    pub(crate) fn drain_block_detail(&mut self) {
        if let Ok(detail) = self.detail_rx.try_recv() {
            self.block_detail = Some(detail);
        }
    }

    pub(crate) fn close_block_detail(&mut self) {
        self.block_detail = None;
    }
}

fn truncate_hex(s: &str) -> String {
    if s.len() > 10 {
        format!("{}..{}", &s[..6], &s[s.len() - 4..])
    } else {
        s.to_string()
    }
}

fn format_ether(wei: U256) -> String {
    let ether_unit = U256::from(10u64).pow(U256::from(18));
    let whole = wei / ether_unit;
    let remainder = wei % ether_unit;

    let frac_digits = 4;
    let frac_unit = U256::from(10u64).pow(U256::from(18 - frac_digits));
    let frac = remainder / frac_unit;

    let frac_val: u64 = frac.try_into().unwrap_or(0);
    let formatted = format!("{whole}.{frac_val:0>4}");
    // Trim trailing zeros but keep at least one decimal
    let trimmed = formatted.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    format!("{trimmed} ETH")
}

pub(crate) fn spawn_balance_poller(
    rpc_url: String,
    accounts: Vec<(String, String)>,
    tx: mpsc::Sender<Vec<String>>,
) {
    let addresses: Vec<Address> = accounts
        .iter()
        .filter_map(|(addr, _)| addr.parse().ok())
        .collect();

    tokio::spawn(async move {
        use alloy_provider::{Provider, ProviderBuilder};

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            interval.tick().await;

            let provider =
                ProviderBuilder::new().connect_http(rpc_url.parse().expect("valid RPC URL"));

            let mut balances = Vec::with_capacity(addresses.len());
            for addr in &addresses {
                match provider.get_balance(*addr).await {
                    Ok(bal) => balances.push(format_ether(bal)),
                    Err(_) => balances.push("? ETH".to_string()),
                }
            }

            if tx.send(balances).await.is_err() {
                break;
            }
        }
    });
}
