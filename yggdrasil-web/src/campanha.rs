//! Apoio à campanha — crowdfunding **independente** (não-Catarse) (YG-161).
//!
//! Modelamos o financiamento como um *ledger* próprio: cada apoio é um `pledge`
//! a um tier (Semente…Yggdrasil), vindo de um usuário logado (`user_sub`) ou
//! anônimo. Nenhuma cobrança acontece aqui — o pledge nasce `pendente` e a
//! confirmação (PIX/transferência, fora de banda) é registrada pelo time via
//! rota admin; só então creditamos as sementes do tier. Independente de Catarse:
//! os tiers, o ledger e a lista de créditos vivem no próprio Yggdrasil.
//!
//! `e-mail`/`user_sub` **nunca** saem na lista pública de créditos (espelha o
//! cuidado de [`crate::feedback`]). Schema criado idempotentemente no boot.

use std::sync::{Arc, Mutex};

use nanoid::nanoid;
use rusqlite::{Connection, params};
use serde::Serialize;

/// Um tier de apoio. `preco` em BRL; `sementes` é a moeda interna creditada na
/// confirmação (espelha `docs/REWARDS.md`: Raiz dá 1.000 e os tiers acima
/// herdam "tudo da Raiz", logo também 1.000). Fonte canônica da prosa segue no
/// REWARDS.md; aqui guardamos só o que o backend precisa (preço + sementes).
#[derive(Serialize, Clone, Copy)]
pub struct Tier {
    pub slug: &'static str,
    pub nome: &'static str,
    pub preco: u32,
    pub sementes: u64,
}

/// Tiers da campanha (allowlist canônica). Ordem = ordem de exibição.
pub const TIERS: [Tier; 6] = [
    Tier {
        slug: "semente",
        nome: "Semente",
        preco: 25,
        sementes: 0,
    },
    Tier {
        slug: "raiz",
        nome: "Raiz",
        preco: 60,
        sementes: 1_000,
    },
    Tier {
        slug: "galho",
        nome: "Galho",
        preco: 120,
        sementes: 1_000,
    },
    Tier {
        slug: "folhagem",
        nome: "Folhagem",
        preco: 250,
        sementes: 1_000,
    },
    Tier {
        slug: "tronco",
        nome: "Tronco",
        preco: 500,
        sementes: 1_000,
    },
    Tier {
        slug: "yggdrasil",
        nome: "Yggdrasil",
        preco: 1_500,
        sementes: 1_000,
    },
];

/// Acha um tier pelo slug (case-sensitive; slugs são canônicos minúsculos).
pub fn tier(slug: &str) -> Option<&'static Tier> {
    TIERS.iter().find(|t| t.slug == slug)
}

/// Status de um pledge. `pendente` ao nascer; `confirmado` após o time
/// registrar o pagamento (fora de banda) pela rota admin.
pub const STATUS_PENDENTE: &str = "pendente";
pub const STATUS_CONFIRMADO: &str = "confirmado";

/// Migração idempotente. Seguro chamar em todo boot.
pub fn init_campanha_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pledges (
            id                TEXT    PRIMARY KEY,
            tier              TEXT    NOT NULL,
            valor             INTEGER NOT NULL,
            nome              TEXT,
            email             TEXT,
            user_sub          TEXT,
            mensagem          TEXT,
            mostrar_creditos  INTEGER NOT NULL DEFAULT 1,
            status            TEXT    NOT NULL DEFAULT 'pendente',
            created_at        INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_pledges_created ON pledges(created_at);
        CREATE INDEX IF NOT EXISTS idx_pledges_tier ON pledges(tier);",
    )?;
    Ok(())
}

/// Dados de um novo apoio (já validados pela rota).
pub struct NewPledge<'a> {
    pub tier: &'a str,
    /// Valor em BRL — snapshot do preço do tier no momento do apoio.
    pub valor: u32,
    pub nome: Option<&'a str>,
    pub email: Option<&'a str>,
    /// `Some(sub)` se logado; `None` = anônimo.
    pub user_sub: Option<&'a str>,
    /// Recado público opcional (aparece nos créditos).
    pub mensagem: Option<&'a str>,
    /// Opt-in para aparecer na página de créditos.
    pub mostrar_creditos: bool,
}

/// Linha exibível em `/creditos`. **Nunca** inclui `email` nem `user_sub`.
#[derive(Serialize)]
pub struct Credito {
    pub tier: String,
    /// `None` quando sem nome → exibido como "Apoiador anônimo".
    pub nome: Option<String>,
    pub mensagem: Option<String>,
    /// `true` quando o pagamento já foi confirmado pelo time.
    pub confirmado: bool,
    pub created_at: i64,
}

/// Linha **completa** de um pledge — visão do operador (YG-165). Inclui
/// `email`/`user_sub`: só sai pela rota admin (`GET /pledges`, gated por token),
/// nunca pelas rotas públicas.
#[derive(Serialize)]
pub struct PledgeFull {
    pub id: String,
    pub tier: String,
    pub valor: u32,
    pub nome: Option<String>,
    pub email: Option<String>,
    pub user_sub: Option<String>,
    pub mensagem: Option<String>,
    pub mostrar_creditos: bool,
    pub status: String,
    pub created_at: i64,
}

pub struct PledgeDb {
    db: Arc<Mutex<Connection>>,
}

impl PledgeDb {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        init_campanha_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        init_campanha_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Registra o apoio (status `pendente`) e devolve o `id` gerado.
    pub fn apoiar(&self, p: &NewPledge) -> String {
        let id = nanoid!();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO pledges
             (id, tier, valor, nome, email, user_sub, mensagem, mostrar_creditos, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id, p.tier, p.valor, p.nome, p.email, p.user_sub, p.mensagem,
                i64::from(p.mostrar_creditos), STATUS_PENDENTE, now_ms
            ],
        );
        id
    }

    /// Créditos públicos: apoios que optaram por aparecer (`mostrar_creditos`),
    /// mais recentes primeiro. Seleciona **apenas** colunas públicas — `email` e
    /// `user_sub` jamais são lidos, então não há como vazarem.
    pub fn creditos(&self) -> Vec<Credito> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT tier, nome, mensagem, status, created_at
             FROM pledges WHERE mostrar_creditos = 1 ORDER BY created_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |r| {
            Ok(Credito {
                tier: r.get(0)?,
                nome: r.get::<_, Option<String>>(1)?,
                mensagem: r.get::<_, Option<String>>(2)?,
                confirmado: r.get::<_, String>(3)? == STATUS_CONFIRMADO,
                created_at: r.get(4)?,
            })
        });
        match rows {
            Ok(it) => it.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Confirma o pagamento de um pledge. Devolve `Some((tier, user_sub))` se a
    /// linha existia (para a rota creditar as sementes do tier ao usuário, se
    /// logado); `None` se o `id` não existe. Idempotente: reconfirmar não
    /// duplica (mas a rota deve evitar recreditar — ver `was_pendente`).
    pub fn confirmar(&self, id: &str) -> Option<ConfirmInfo> {
        let conn = self.db.lock().unwrap();
        let prev: Option<(String, Option<String>, String)> = conn
            .query_row(
                "SELECT tier, user_sub, status FROM pledges WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .ok();
        let (tier, user_sub, status) = prev?;
        let was_pendente = status != STATUS_CONFIRMADO;
        let _ = conn.execute(
            "UPDATE pledges SET status = ?1 WHERE id = ?2",
            params![STATUS_CONFIRMADO, id],
        );
        Some(ConfirmInfo {
            tier,
            user_sub,
            was_pendente,
        })
    }

    /// Agregados públicos da campanha (YG-164): total **confirmado** em BRL,
    /// nº de apoios confirmados e nº de pendentes ("em processamento"). Conta
    /// **só confirmados** no arrecadado — o número não infla com intenções não
    /// pagas. Sem PII (só somas/contagens).
    pub fn progresso(&self) -> Progresso {
        let conn = self.db.lock().unwrap();
        let arrecadado: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(valor), 0) FROM pledges WHERE status = ?1",
                params![STATUS_CONFIRMADO],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let apoiadores: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pledges WHERE status = ?1",
                params![STATUS_CONFIRMADO],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let pendentes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pledges WHERE status = ?1",
                params![STATUS_PENDENTE],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Progresso {
            arrecadado: arrecadado.max(0) as u64,
            apoiadores: apoiadores.max(0) as u64,
            pendentes: pendentes.max(0) as u64,
        }
    }

    /// Todos os pledges, completos (YG-165) — visão do operador (admin-only na
    /// rota). Mais recentes primeiro. Devolve `email`/`user_sub`: **só** use por
    /// trás do admin token.
    pub fn list_all(&self) -> Vec<PledgeFull> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, tier, valor, nome, email, user_sub, mensagem,
                    mostrar_creditos, status, created_at
             FROM pledges ORDER BY created_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |r| {
            Ok(PledgeFull {
                id: r.get(0)?,
                tier: r.get(1)?,
                valor: r.get::<_, i64>(2)? as u32,
                nome: r.get(3)?,
                email: r.get(4)?,
                user_sub: r.get(5)?,
                mensagem: r.get(6)?,
                mostrar_creditos: r.get::<_, i64>(7)? == 1,
                status: r.get(8)?,
                created_at: r.get(9)?,
            })
        });
        match rows {
            Ok(it) => it.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// Agregados da campanha (YG-164) — sem PII.
#[derive(Serialize, Default)]
pub struct Progresso {
    /// Total arrecadado (BRL) — soma do `valor` dos apoios **confirmados**.
    pub arrecadado: u64,
    /// Nº de apoios confirmados.
    pub apoiadores: u64,
    /// Nº de apoios `pendente` (registrados, pagamento ainda não confirmado).
    pub pendentes: u64,
}

/// Resultado de confirmar um pledge.
pub struct ConfirmInfo {
    pub tier: String,
    pub user_sub: Option<String>,
    /// `true` se estava `pendente` (i.e. esta foi a primeira confirmação) —
    /// a rota só credita sementes quando `was_pendente`, para não recreditar.
    pub was_pendente: bool,
}

// ── Test helpers ─────────────────────────────────────────────────────────────

#[cfg(test)]
impl PledgeDb {
    pub fn count(&self) -> i64 {
        let conn = self.db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM pledges", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// (tier, valor, status, user_sub, mostrar_creditos) da linha `id`.
    pub fn row(&self, id: &str) -> Option<(String, u32, String, Option<String>, bool)> {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT tier, valor, status, user_sub, mostrar_creditos FROM pledges WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? as u32,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)? == 1,
                ))
            },
        )
        .ok()
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_idempotente() {
        let db = PledgeDb::in_memory().unwrap();
        let conn = db.db.lock().unwrap();
        init_campanha_db(&conn).unwrap(); // segunda chamada não falha
    }

    #[test]
    fn tiers_canonicos_e_lookup() {
        assert_eq!(TIERS.len(), 6);
        assert_eq!(tier("semente").unwrap().preco, 25);
        assert_eq!(tier("raiz").unwrap().sementes, 1_000);
        assert_eq!(tier("yggdrasil").unwrap().preco, 1_500);
        assert!(tier("inexistente").is_none());
    }

    #[test]
    fn apoiar_pendente_grava_e_conta() {
        let db = PledgeDb::in_memory().unwrap();
        let id = db.apoiar(&NewPledge {
            tier: "raiz",
            valor: 60,
            nome: Some("Ana"),
            email: Some("ana@x.com"),
            user_sub: Some("u-1"),
            mensagem: Some("Vamos juntos!"),
            mostrar_creditos: true,
        });
        let (tier, valor, status, sub, mostrar) = db.row(&id).unwrap();
        assert_eq!(tier, "raiz");
        assert_eq!(valor, 60);
        assert_eq!(status, STATUS_PENDENTE);
        assert_eq!(sub.as_deref(), Some("u-1"));
        assert!(mostrar);
        assert_eq!(db.count(), 1);
    }

    #[test]
    fn creditos_so_traz_opt_in_e_nunca_email() {
        let db = PledgeDb::in_memory().unwrap();
        db.apoiar(&NewPledge {
            tier: "semente",
            valor: 25,
            nome: Some("Visível"),
            email: Some("vis@x.com"),
            user_sub: None,
            mensagem: None,
            mostrar_creditos: true,
        });
        db.apoiar(&NewPledge {
            tier: "raiz",
            valor: 60,
            nome: Some("Oculto"),
            email: Some("oc@x.com"),
            user_sub: Some("u-2"),
            mensagem: None,
            mostrar_creditos: false,
        });
        let cred = db.creditos();
        assert_eq!(cred.len(), 1, "só o opt-in aparece");
        assert_eq!(cred[0].nome.as_deref(), Some("Visível"));
        assert!(!cred[0].confirmado, "nasce pendente");
        let json = serde_json::to_string(&cred).unwrap();
        assert!(!json.contains("vis@x.com"));
        assert!(!json.contains("email"));
        assert!(!json.contains("user_sub"));
    }

    #[test]
    fn confirmar_marca_e_reporta_tier_e_user() {
        let db = PledgeDb::in_memory().unwrap();
        let id = db.apoiar(&NewPledge {
            tier: "raiz",
            valor: 60,
            nome: Some("Ana"),
            email: None,
            user_sub: Some("u-1"),
            mensagem: None,
            mostrar_creditos: true,
        });
        let info = db.confirmar(&id).expect("linha existe");
        assert_eq!(info.tier, "raiz");
        assert_eq!(info.user_sub.as_deref(), Some("u-1"));
        assert!(info.was_pendente, "primeira confirmação");
        // reflete na lista de créditos
        assert!(db.creditos()[0].confirmado);
        // reconfirmar não conta como primeira vez (não recredita)
        let again = db.confirmar(&id).unwrap();
        assert!(!again.was_pendente);
    }

    #[test]
    fn confirmar_id_inexistente_none() {
        let db = PledgeDb::in_memory().unwrap();
        assert!(db.confirmar("nao-existe").is_none());
    }

    #[test]
    fn progresso_conta_so_confirmados() {
        let db = PledgeDb::in_memory().unwrap();
        let np = |tier: &'static str, valor: u32, sub: &'static str| NewPledge {
            tier,
            valor,
            nome: None,
            email: None,
            user_sub: Some(sub),
            mensagem: None,
            mostrar_creditos: true,
        };
        let a = db.apoiar(&np("raiz", 60, "u1"));
        let b = db.apoiar(&np("galho", 120, "u2"));
        let _c = db.apoiar(&np("semente", 25, "u3")); // fica pendente

        // nada confirmado ainda
        let p0 = db.progresso();
        assert_eq!(p0.arrecadado, 0);
        assert_eq!(p0.apoiadores, 0);
        assert_eq!(p0.pendentes, 3);

        db.confirmar(&a);
        db.confirmar(&b);
        let p = db.progresso();
        assert_eq!(p.arrecadado, 180, "60 + 120 confirmados");
        assert_eq!(p.apoiadores, 2);
        assert_eq!(p.pendentes, 1);
    }
}
