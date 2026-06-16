//! PIX BR Code ("copia e cola") + QR — pagamento **independente** (YG-163).
//!
//! Gera o payload EMV®/BR Code do PIX localmente, a partir de uma **chave PIX**
//! configurada (`YGGDRASIL_PIX_KEY`) — sem PSP, sem conta de gateway, sem
//! webhook. O apoiador cola o código (ou lê o QR) no app do banco e paga direto
//! à chave do recebedor. A confirmação segue **manual** (rota admin
//! `confirmar`): casa com o ledger `pendente → confirmado` da [`crate::campanha`].
//!
//! Referência: BR Code (EMVCo MPM) + Manual de Padrões do BCB. O CRC é
//! CRC-16/CCITT-FALSE (poly `0x1021`, init `0xFFFF`).

/// Config do recebedor PIX (do ambiente). `None` em qualquer campo essencial →
/// PIX desligado (a campanha cai no texto "instruções em breve").
#[derive(Clone)]
pub struct PixConfig {
    pub key: String,
    pub merchant_name: String,
    pub merchant_city: String,
}

impl PixConfig {
    /// Lê do ambiente: `YGGDRASIL_PIX_KEY` (obrigatória), `YGGDRASIL_PIX_NAME`
    /// (default "YGGDRASIL"), `YGGDRASIL_PIX_CITY` (default "SAO PAULO").
    /// `None` se a chave não estiver configurada → feature desligada.
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("YGGDRASIL_PIX_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        Some(Self {
            key: key.trim().to_string(),
            merchant_name: std::env::var("YGGDRASIL_PIX_NAME")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "YGGDRASIL".to_string()),
            merchant_city: std::env::var("YGGDRASIL_PIX_CITY")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "SAO PAULO".to_string()),
        })
    }
}

/// Um campo TLV do BR Code: `ID(2) + LEN(2) + VALUE`. `len` é a contagem de
/// caracteres do valor (ASCII no BR Code, então == bytes após sanitização).
fn tlv(id: &str, value: &str) -> String {
    format!("{id}{:02}{value}", value.chars().count())
}

/// Remove acentos/não-ASCII e limita o tamanho — nome (≤25) e cidade (≤15) do
/// BR Code devem ser ASCII imprimível. Mantém letras, dígitos, espaço e alguns
/// sinais; o resto vira espaço, depois colapsa/trim.
fn ascii_clean(s: &str, max: usize) -> String {
    let folded: String = s
        .chars()
        .map(|c| {
            let l = c.to_ascii_uppercase();
            if l.is_ascii_alphanumeric() || l == ' ' {
                l
            } else {
                match c {
                    'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'á' | 'à' | 'â' | 'ã' | 'ä' => 'A',
                    'É' | 'Ê' | 'Ë' | 'é' | 'ê' | 'ë' => 'E',
                    'Í' | 'Î' | 'Ï' | 'í' | 'î' | 'ï' => 'I',
                    'Ó' | 'Ô' | 'Õ' | 'Ö' | 'ó' | 'ô' | 'õ' | 'ö' => 'O',
                    'Ú' | 'Û' | 'Ü' | 'ú' | 'û' | 'ü' => 'U',
                    'Ç' | 'ç' => 'C',
                    _ => ' ',
                }
            }
        })
        .collect();
    folded
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// Sanitiza o txid: alfanumérico, ≤25 chars; vazio vira `***` (padrão BR Code).
fn clean_txid(txid: &str) -> String {
    let t: String = txid
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(25)
        .collect();
    if t.is_empty() { "***".to_string() } else { t }
}

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF) sobre os bytes — o checksum do
/// BR Code, calculado sobre todo o payload incluindo "6304". Valor de teste
/// padrão: CRC("123456789") == 0x29B1.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Monta o BR Code ("copia e cola") estático para um apoio. `amount_brl` em
/// reais inteiros (preço do tier); `txid` = referência (ex.: id do pledge).
pub fn br_code(cfg: &PixConfig, amount_brl: u32, txid: &str) -> String {
    // Merchant Account Information (26): GUI + chave PIX.
    let gui = tlv("00", "br.gov.bcb.pix");
    let key = tlv("01", cfg.key.trim());
    let mai = tlv("26", &format!("{gui}{key}"));

    // Additional Data Field (62): txid (05).
    let add = tlv("62", &tlv("05", &clean_txid(txid)));

    let amount = format!("{amount_brl}.00");

    let mut payload = String::new();
    payload.push_str(&tlv("00", "01")); // Payload Format Indicator
    payload.push_str(&tlv("01", "11")); // Point of Initiation: estático/reutilizável
    payload.push_str(&mai);
    payload.push_str(&tlv("52", "0000")); // MCC
    payload.push_str(&tlv("53", "986")); // moeda: BRL
    payload.push_str(&tlv("54", &amount)); // valor
    payload.push_str(&tlv("58", "BR")); // país
    payload.push_str(&tlv("59", &ascii_clean(&cfg.merchant_name, 25)));
    payload.push_str(&tlv("60", &ascii_clean(&cfg.merchant_city, 15)));
    payload.push_str(&add);
    payload.push_str("6304"); // ID+LEN do CRC; o CRC cobre até aqui

    let crc = crc16(payload.as_bytes());
    payload.push_str(&format!("{crc:04X}"));
    payload
}

/// QR do BR Code como `<svg>` (string), para embutir inline na página. `None` se
/// o conteúdo não couber num QR (não deve acontecer com um BR Code de campanha).
pub fn qr_svg(br_code: &str) -> Option<String> {
    use qrcode::QrCode;
    use qrcode::render::svg;
    let code = QrCode::new(br_code.as_bytes()).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PixConfig {
        PixConfig {
            key: "yuri@artelonga.com.br".to_string(),
            merchant_name: "Yggdrasil Coração".to_string(),
            merchant_city: "São Paulo".to_string(),
        }
    }

    #[test]
    fn crc16_valor_de_teste_padrao() {
        // check value canônico do CRC-16/CCITT-FALSE
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn br_code_tem_campos_essenciais_e_crc_consistente() {
        let code = br_code(&cfg(), 60, "pledge123");
        assert!(code.starts_with("000201"), "Payload Format Indicator");
        assert!(code.contains("br.gov.bcb.pix"));
        assert!(code.contains("yuri@artelonga.com.br"));
        assert!(code.contains("5303986"), "moeda BRL");
        assert!(code.contains("540560.00"), "valor 60.00 com len 05");
        assert!(code.contains("5802BR"));
        assert!(code.contains("pledge123"), "txid presente");

        // CRC self-check: recomputa sobre tudo até "6304" e compara com o sufixo
        let (head, crc_hex) = code.split_at(code.len() - 4);
        assert_eq!(format!("{:04X}", crc16(head.as_bytes())), crc_hex);
    }

    #[test]
    fn ascii_clean_remove_acentos_e_limita() {
        assert_eq!(ascii_clean("São Paulo", 15), "SAO PAULO");
        assert_eq!(ascii_clean("Coração", 25), "CORACAO");
        assert_eq!(ascii_clean("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 5), "AAAAA");
    }

    #[test]
    fn txid_vazio_vira_estrela_e_alfanumerico() {
        assert_eq!(clean_txid(""), "***");
        assert_eq!(clean_txid("ab_12-Xy!"), "ab12Xy");
    }

    #[test]
    fn merchant_name_acentuado_nao_quebra_o_len() {
        // o nome "Yggdrasil Coração" vira ASCII; o len do campo 59 == nº de chars
        let code = br_code(&cfg(), 25, "x");
        // localiza "59" + len + value e confere que o len casa com o conteúdo ASCII
        let idx = code.find("5917").or_else(|| code.find("59")).unwrap();
        // "Yggdrasil Coracao" tem 17 chars
        assert!(
            code[idx..].starts_with("5917YGGDRASIL CORACAO"),
            "campo 59 bem-formado: {}",
            &code[idx..idx + 25]
        );
    }

    #[test]
    fn qr_svg_gera_svg() {
        let code = br_code(&cfg(), 60, "p1");
        let svg = qr_svg(&code).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn pix_config_desligado_sem_chave() {
        // sem env var → None (feature desligada). Não dá pra setar env aqui de
        // forma isolada; validamos o caminho de presença via construção direta.
        let c = PixConfig {
            key: "  ".to_string(),
            merchant_name: "x".to_string(),
            merchant_city: "y".to_string(),
        };
        // chave em branco trim → br_code ainda monta, mas a rota usa from_env
        // que filtra branco; aqui só garantimos que trim não deixa espaço no 01.
        let code = br_code(&c, 10, "t");
        assert!(code.contains("0100"), "chave vazia vira len 00");
    }
}
