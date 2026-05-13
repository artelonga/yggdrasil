use std::sync::Arc;

use game_core::mail::MailProvider;
use lettre::{Message, SmtpTransport, Transport, transport::smtp::authentication::Credentials};

pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub from: String,
}

impl SmtpConfig {
    /// Lê config SMTP das variáveis de ambiente.
    /// Retorna `None` se `YGGDRASIL_SMTP_HOST` não estiver definido.
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("YGGDRASIL_SMTP_HOST")
            .ok()
            .filter(|s| !s.is_empty())?;
        let port = std::env::var("YGGDRASIL_SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587u16);
        let user = std::env::var("YGGDRASIL_SMTP_USER").unwrap_or_default();
        let password = std::env::var("YGGDRASIL_SMTP_PASSWORD").unwrap_or_default();
        let from = std::env::var("YGGDRASIL_SMTP_FROM")
            .unwrap_or_else(|_| "Yggdrasil <noreply@artelonga.com.br>".to_string());
        Some(Self {
            host,
            port,
            user,
            password,
            from,
        })
    }
}

pub struct SmtpMailProvider {
    mailer: SmtpTransport,
    from: lettre::message::Mailbox,
}

impl SmtpMailProvider {
    pub fn from_config(config: &SmtpConfig) -> Result<Self, String> {
        let from = config
            .from
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| format!("SMTP `from` inválido: {e}"))?;

        let creds = Credentials::new(config.user.clone(), config.password.clone());

        let mailer = SmtpTransport::starttls_relay(&config.host)
            .map_err(|e| format!("SMTP relay inválido: {e}"))?
            .credentials(creds)
            .port(config.port)
            .build();

        Ok(Self { mailer, from })
    }
}

impl MailProvider for SmtpMailProvider {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), String> {
        let to_addr = to
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| format!("endereço de destino inválido: {e}"))?;

        let email = Message::builder()
            .from(self.from.clone())
            .to(to_addr)
            .subject(subject)
            .body(body.to_string())
            .map_err(|e| format!("erro ao construir email: {e}"))?;

        // block_in_place sinaliza ao tokio multi-thread que esta thread vai bloquear.
        tokio::task::block_in_place(|| {
            self.mailer
                .send(&email)
                .map(|_| ())
                .map_err(|e| format!("erro ao enviar email via SMTP: {e}"))
        })
    }
}

/// Constrói o mail provider adequado baseado nas variáveis de ambiente.
/// Quando `YGGDRASIL_SMTP_HOST` não está definido, usa `LogMailProvider` (modo dev).
pub fn build_mail_provider() -> Arc<dyn MailProvider> {
    match SmtpConfig::from_env() {
        None => {
            tracing::warn!("smtp not configured — emails go to stdout");
            Arc::new(game_core::mail::LogMailProvider)
        }
        Some(config) => match SmtpMailProvider::from_config(&config) {
            Ok(provider) => {
                tracing::info!(host = %config.host, port = config.port, "SMTP configurado");
                Arc::new(provider)
            }
            Err(e) => {
                tracing::warn!("smtp not configured — emails go to stdout (erro de config: {e})");
                Arc::new(game_core::mail::LogMailProvider)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtp_provider_construido_com_config_valida() {
        let config = SmtpConfig {
            host: "sandbox.smtp.mailtrap.io".to_string(),
            port: 587,
            user: "api-user".to_string(),
            password: "api-key".to_string(),
            from: "Yggdrasil <noreply@artelonga.com.br>".to_string(),
        };
        assert!(SmtpMailProvider::from_config(&config).is_ok());
    }

    #[test]
    fn smtp_provider_rejeita_from_invalido() {
        let config = SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            user: String::new(),
            password: String::new(),
            from: "não é um email válido".to_string(),
        };
        assert!(SmtpMailProvider::from_config(&config).is_err());
    }

    #[test]
    fn smtp_config_porta_default_587() {
        let config = SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            user: String::new(),
            password: String::new(),
            from: "Yggdrasil <noreply@artelonga.com.br>".to_string(),
        };
        assert_eq!(config.port, 587);
    }

    #[test]
    fn smtp_config_from_env_retorna_none_sem_host() {
        if std::env::var("YGGDRASIL_SMTP_HOST").is_err() {
            assert!(SmtpConfig::from_env().is_none());
        }
    }

    /// Teste de integração com Mailtrap (sandbox SMTP).
    ///
    /// Executar com:
    ///   YGGDRASIL_SMTP_HOST=sandbox.smtp.mailtrap.io \
    ///   YGGDRASIL_SMTP_USER=<user> \
    ///   YGGDRASIL_SMTP_PASSWORD=<pass> \
    ///   YGGDRASIL_SMTP_TEST_TO=<to> \
    ///   cargo test smtp_envia_email_via_sandbox -- --ignored --nocapture
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requer servidor SMTP configurado (Mailtrap, SendGrid, etc.)"]
    async fn smtp_envia_email_via_sandbox() {
        let config = SmtpConfig {
            host: std::env::var("YGGDRASIL_SMTP_HOST").expect("YGGDRASIL_SMTP_HOST"),
            port: std::env::var("YGGDRASIL_SMTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(587),
            user: std::env::var("YGGDRASIL_SMTP_USER").expect("YGGDRASIL_SMTP_USER"),
            password: std::env::var("YGGDRASIL_SMTP_PASSWORD").expect("YGGDRASIL_SMTP_PASSWORD"),
            from: std::env::var("YGGDRASIL_SMTP_FROM")
                .unwrap_or_else(|_| "Yggdrasil <noreply@artelonga.com.br>".to_string()),
        };
        let to = std::env::var("YGGDRASIL_SMTP_TEST_TO").expect("YGGDRASIL_SMTP_TEST_TO");

        let provider = SmtpMailProvider::from_config(&config).expect("config válida");
        let result = provider.send(
            &to,
            "Teste YG-24 — SmtpMailProvider",
            "Email de teste gerado pela suite de integração do Yggdrasil.",
        );
        assert!(result.is_ok(), "erro: {:?}", result.err());
    }
}
