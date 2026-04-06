use std::sync::atomic::{AtomicU32, Ordering};

use zimppy_wallet::{WalletBalance, WalletConfig, WalletError, ZimppyWallet};

/// Pool that rotates across N accounts within a single wallet.
///
/// Each account has independent note pools (ZIP32), so back-to-back sends
/// don't require waiting for confirmations on a single account's notes.
///
/// **Note on concurrency**: All accounts share a single `ZimppyWallet` (and
/// thus a single `LightClient`). Sends are serialized through the wallet's
/// internal lock. The pool provides **note-pool isolation** (account A's notes
/// remain spendable while account B's are pending confirmation), not concurrent
/// send execution. The atomic counter wraps at `u32::MAX` which is safe
/// (modulo brings it back to account 0).
pub struct AccountPool {
    wallet: ZimppyWallet,
    num_accounts: u32,
    next: AtomicU32,
}

impl AccountPool {
    /// Create a pool backed by a wallet with N accounts.
    /// If the wallet has fewer than `num_accounts`, new accounts are created.
    pub async fn new(wallet_config: WalletConfig, num_accounts: u32) -> Result<Self, WalletError> {
        let num_accounts = num_accounts.max(1);
        let mut wallet = ZimppyWallet::open(WalletConfig {
            num_accounts,
            ..wallet_config
        })
        .await?;

        // Ensure we have enough accounts
        let existing = wallet.num_accounts().await;
        for _ in existing..num_accounts {
            wallet.create_account().await?;
        }

        Ok(Self {
            wallet,
            num_accounts,
            next: AtomicU32::new(0),
        })
    }

    /// Get the next account index (round-robin).
    pub fn next_account(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) % self.num_accounts
    }

    /// Sync the wallet (covers all accounts).
    pub async fn sync(&mut self) -> Result<(), WalletError> {
        self.wallet.sync().await?;
        Ok(())
    }

    /// Send from the next available account.
    pub async fn send(
        &mut self,
        to: &str,
        amount_zat: u64,
        memo: Option<&str>,
    ) -> Result<String, WalletError> {
        let account = self.next_account();
        self.wallet.send_from_account(account, to, amount_zat, memo).await
    }

    /// Get balances for all accounts.
    pub async fn all_balances(&self) -> Result<Vec<(u32, WalletBalance)>, WalletError> {
        let mut result = Vec::new();
        for i in 0..self.num_accounts {
            let bal = self.wallet.balance_for_account(i).await?;
            result.push((i, bal));
        }
        Ok(result)
    }

    /// Get the underlying wallet reference.
    pub fn wallet(&self) -> &ZimppyWallet {
        &self.wallet
    }

    /// Get mutable wallet reference.
    pub fn wallet_mut(&mut self) -> &mut ZimppyWallet {
        &mut self.wallet
    }

    /// Number of accounts in the pool.
    pub fn num_accounts(&self) -> u32 {
        self.num_accounts
    }
}
