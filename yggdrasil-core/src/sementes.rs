use game_core::storage::{Storage, WalletManager};
use std::sync::Arc;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SementesError>;

#[derive(Debug, Error)]
pub enum SementesError {
    #[error("Saldo insuficiente")]
    SaldoInsuficiente,
    #[error("Erro no engine: {0}")]
    EngineError(String),
}

impl From<game_core::engine::error::GameError> for SementesError {
    fn from(e: game_core::engine::error::GameError) -> Self {
        SementesError::EngineError(e.to_string())
    }
}

/// Fachada de domínio sobre [`WalletManager`] usando terminologia Yggdrasil.
///
/// Todas as APIs públicas falam "sementes" e "saldo"; o `WalletManager` do engine
/// nunca aparece na superfície pública.
pub struct SaldoInfo {
    pub saldo: u64,
    pub atualizado_em: chrono::DateTime<chrono::Utc>,
}

pub struct Sementes {
    storage: Arc<Storage>,
}

impl Sementes {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    fn inner(&self) -> WalletManager<'_> {
        WalletManager::new(&self.storage)
    }

    /// Retorna o saldo atual de sementes do usuário.
    pub fn saldo(&self, _user_id: &str) -> Result<u64> {
        self.inner().get_balance().map_err(Into::into)
    }

    /// Retorna saldo e timestamp de última atualização para o usuário.
    pub fn saldo_info(&self, user_id: &str) -> Result<SaldoInfo> {
        match self
            .storage
            .get_wallet_for_user(user_id)
            .map_err(SementesError::from)?
        {
            Some(wallet) => Ok(SaldoInfo {
                saldo: wallet.balance,
                atualizado_em: chrono::DateTime::from_timestamp(wallet.last_updated, 0)
                    .unwrap_or_else(chrono::Utc::now),
            }),
            None => Ok(SaldoInfo {
                saldo: 0,
                atualizado_em: chrono::Utc::now(),
            }),
        }
    }

    /// Credita `qtd` sementes ao usuário.
    pub fn creditar(&self, user_id: &str, qtd: u64) -> Result<()> {
        let qtd_u32 = qtd.min(u32::MAX as u64) as u32;
        self.inner()
            .cash_out(user_id, qtd_u32, 0)
            .map_err(Into::into)
    }

    /// Debita `qtd` sementes do usuário. Retorna o saldo restante.
    ///
    /// Erro [`SementesError::SaldoInsuficiente`] se o saldo for menor que `qtd`.
    pub fn debitar(&self, user_id: &str, qtd: u64) -> Result<u64> {
        let saldo_atual = self.saldo(user_id)?;
        if saldo_atual < qtd {
            return Err(SementesError::SaldoInsuficiente);
        }
        let qtd_u32 = qtd.min(u32::MAX as u64) as u32;
        self.inner()
            .buy_in(user_id, qtd_u32)
            .map_err(SementesError::from)?;
        self.saldo(user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::storage::schema;
    use tempfile::tempdir;

    fn make_sementes(balance: u64) -> (Sementes, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());
        let wallet = schema::Wallet {
            user_id: "user1".to_string(),
            balance,
            last_updated: 0,
        };
        storage.save_wallet(&wallet).unwrap();
        (Sementes::new(storage), dir)
    }

    #[test]
    fn saldo_retorna_balance_correto() {
        let (s, _dir) = make_sementes(1_000);
        assert_eq!(s.saldo("user1").unwrap(), 1_000);
    }

    #[test]
    fn creditar_aumenta_saldo() {
        let (s, _dir) = make_sementes(500);
        s.creditar("user1", 300).unwrap();
        assert_eq!(s.saldo("user1").unwrap(), 800);
    }

    #[test]
    fn debitar_diminui_saldo_e_retorna_restante() {
        let (s, _dir) = make_sementes(1_000);
        let restante = s.debitar("user1", 400).unwrap();
        assert_eq!(restante, 600);
    }

    #[test]
    fn debitar_saldo_insuficiente_retorna_erro() {
        let (s, _dir) = make_sementes(100);
        let err = s.debitar("user1", 200).unwrap_err();
        assert!(matches!(err, SementesError::SaldoInsuficiente));
    }

    #[test]
    fn debitar_exato_deixa_saldo_zero() {
        let (s, _dir) = make_sementes(500);
        let restante = s.debitar("user1", 500).unwrap();
        assert_eq!(restante, 0);
    }

    #[test]
    fn creditar_depois_de_debitar() {
        let (s, _dir) = make_sementes(1_000);
        s.debitar("user1", 600).unwrap();
        s.creditar("user1", 200).unwrap();
        assert_eq!(s.saldo("user1").unwrap(), 600);
    }

    #[test]
    fn erro_saldo_insuficiente_mensagem_pt_br() {
        let (s, _dir) = make_sementes(0);
        let err = s.debitar("user1", 1).unwrap_err();
        assert_eq!(err.to_string(), "Saldo insuficiente");
    }
}
