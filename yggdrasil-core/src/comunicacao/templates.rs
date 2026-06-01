//! Templates-semente das salas: Iorubá e Mbyá Guaraní.
//!
//! Cada template é um conjunto de [`Element`]s já posicionados num mapa solto,
//! com glosa, pronúncia, conceito âncora e referência. O usuário herda esse
//! mapa e pode mover, editar, acrescentar e publicar termos novos.
//!
//! Escopo Iorubá: vocabulário litúrgico afro-brasileiro + palavras-núcleo
//! (mesma decisão de escopo de `languages/yo.md` no repo `comunicacao`).
//! Escopo Mbyá: cosmologia e modo de viver (nhandereko), alinhado a
//! `guarani-mbya/` e ao Ayvu Rapyta (Cadogan).

use super::room::{Element, Link, Reference, Room};

/// Resumo de um template para listagem na API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TemplateSummary {
    pub slug: String,
    pub title: String,
    pub lang: String,
    pub description: String,
    pub element_count: usize,
}

/// Slugs de template disponíveis.
pub const TEMPLATES: &[&str] = &["yoruba", "mbya", "blank"];

/// Lista resumida dos templates (para `GET /api/v1/comunicacao/templates`).
pub fn template_summaries() -> Vec<TemplateSummary> {
    TEMPLATES
        .iter()
        .filter_map(|slug| {
            let room = instantiate(slug, "__preview__", "__preview__")?;
            Some(TemplateSummary {
                slug: (*slug).to_string(),
                title: room.title.clone(),
                lang: room.lang.clone(),
                description: template_description(slug).to_string(),
                element_count: room.elements.len(),
            })
        })
        .collect()
}

fn template_description(slug: &str) -> &'static str {
    match slug {
        "yoruba" => {
            "Mapa do vocabulário litúrgico afro-brasileiro de matriz iorubá — \
             orixás, força vital (àṣẹ) e palavras-núcleo."
        }
        "mbya" => {
            "Mapa da cosmologia e do modo de viver Mbyá Guaraní — nhandereko, \
             a alma-palavra (ayvu/nhe'ẽ) e as eras (ara)."
        }
        "blank" => "Sala vazia — comece do zero e acrescente seus próprios termos.",
        _ => "",
    }
}

/// Instancia uma sala a partir do slug do template. `None` se desconhecido.
pub fn instantiate(slug: &str, id: &str, owner: &str) -> Option<Room> {
    #[allow(clippy::type_complexity)]
    let (title, lang, elements, links): (&str, &str, Vec<Element>, Vec<Link>) = match slug {
        "yoruba" => ("Sala Iorubá", "yo", yoruba_elements(), yoruba_links()),
        "mbya" => (
            "Sala Mbyá Guaraní",
            "gn-mbya",
            mbya_elements(),
            mbya_links(),
        ),
        "blank" => ("Nova sala", "pt", Vec::new(), Vec::new()),
        _ => return None,
    };
    let mut room = Room::empty(id, owner, title, lang);
    room.template = slug.to_string();
    room.elements = elements;
    room.links = links;
    Some(room)
}

/// Relações-semente Iorubá (o significado emerge das relações entre termos).
fn yoruba_links() -> Vec<Link> {
    vec![
        Link::new("y1", "olodumare", "ase").with_label("fonte de"),
        Link::new("y2", "ase", "ori").with_label("anima"),
        Link::new("y3", "orixa", "olodumare").with_label("emana de"),
        Link::new("y4", "iya", "iemanja").with_label("arquétipo materno"),
        Link::new("y5", "iemanja", "omi").with_label("domínio"),
        Link::new("y6", "ede", "oro")
            .with_label("relacionado")
            .undirected(),
        // complexo de Ifá
        Link::new("y7", "orunmila", "ifa").with_label("revela"),
        Link::new("y8", "ifa", "odu").with_label("compõe-se de"),
        Link::new("y9", "orunmila", "exu")
            .with_label("parceria")
            .undirected(),
        Link::new("y10", "ori", "egbe").with_label("vínculo com"),
        Link::new("y11", "ori", "orunmila").with_label("destino testemunhado por"),
    ]
}

/// Relações-semente Mbyá Guaraní.
fn mbya_links() -> Vec<Link> {
    vec![
        Link::new("m1", "nhandereko", "teko").with_label("raiz em"),
        Link::new("m2", "nhandereko", "tekoa").with_label("realiza-se em"),
        Link::new("m3", "nhanderu", "ayvu-rapyta").with_label("cria"),
        Link::new("m4", "ayvu-rapyta", "ayvu").with_label("funda"),
        Link::new("m5", "ayvu", "nhee")
            .with_label("alma-palavra")
            .undirected(),
        Link::new("m6", "ayvu-pora", "ayvu").with_label("ideal de"),
        Link::new("m7", "ara", "ara-pyau").with_label("renova em"),
        Link::new("m8", "jaxy", "ara-pyau")
            .with_label("mesmo morfema (pyau)")
            .undirected(),
        Link::new("m9", "yvy", "kaaguy")
            .with_label("abriga")
            .undirected(),
        Link::new("m10", "teko", "yvy").with_label("enraíza-se em"),
    ]
}

// ─── Referências reutilizadas ─────────────────────────────────────────────────

fn ref_ase() -> Reference {
    Reference::new(
        "https://en.wikipedia.org/wiki/A%E1%B9%A3%E1%BA%B9",
        "Wikipedia — Aṣẹ",
        "article",
    )
}

fn ref_ori() -> Reference {
    Reference::new(
        "https://en.wikipedia.org/wiki/Ori_(Yoruba)",
        "Wikipedia — Orí (Yoruba)",
        "article",
    )
}

fn ref_orisha() -> Reference {
    Reference::new(
        "https://en.wikipedia.org/wiki/Orisha",
        "Wikipedia — Orisha",
        "article",
    )
}

fn ref_ayvu_rapyta_yt() -> Reference {
    Reference::new(
        "https://www.youtube.com/watch?v=I1WypSTPubg",
        "AYVU RAPYTA: Creation According to the Mbyá-Guaraní",
        "youtube",
    )
}

fn ref_nhandereko() -> Reference {
    Reference::new(
        "https://clacs.berkeley.edu/nhandereko-guarani-law-audio-annotations",
        "CLACS Berkeley — Nhandereko, the Guarani Law",
        "article",
    )
}

// Referências de vídeo verificadas via yt-dlp (títulos/ids reais do YouTube).

fn ref_nhandereko_yt() -> Reference {
    Reference::new(
        "https://youtu.be/qt-Mmlk-YZc",
        "NHANDEREKO — Aldeia Pyau, Jaraguá/SP (vídeo)",
        "youtube",
    )
}

fn ref_nhamandu_yt() -> Reference {
    Reference::new(
        "https://youtu.be/GCOdIE7D4XM",
        "Nhamandu — canto Mbyá-Guaraní (Morro da Saudade/SP)",
        "youtube",
    )
}

fn ref_ayvu_palavra_alma_yt() -> Reference {
    Reference::new(
        "https://youtu.be/Z0w0PxKL2TQ",
        "Ayvu Rapyta — onde a palavra é alma e a linguagem, princípio vital",
        "youtube",
    )
}

fn ref_iemanja_yt() -> Reference {
    Reference::new(
        "https://youtu.be/78Oh-_4_Zc8",
        "Iemanjá — candomblé: 'eje nile je odo' (ponto cantado)",
        "youtube",
    )
}

fn ref_exu_yt() -> Reference {
    Reference::new(
        "https://youtu.be/QAG5WhOvXvY",
        "Eshu/Elegba, the Orisa who opens the doors — Ella Andall",
        "youtube",
    )
}

fn ref_ede_yt() -> Reference {
    Reference::new(
        "https://youtu.be/WhD1GUGGtEo",
        "Yoruba Alphabet Song (Álífábẹ́ẹ̀tì Èdè Yorùbá)",
        "youtube",
    )
}

// Complexo de Ifá — referências etnográficas em vídeo.

fn ref_olodumare_yt() -> Reference {
    Reference::new(
        "https://youtu.be/aMSuDnvf9vE",
        "Olódùmarè: The Hidden God of Yoruba Cosmology Explained",
        "youtube",
    )
}

fn ref_orunmila_yt() -> Reference {
    Reference::new(
        "https://youtu.be/UH-2QktdwRs",
        "Òrúnmìlà: the man, the orisha and the worldview",
        "youtube",
    )
}

fn ref_ifa_unesco_yt() -> Reference {
    Reference::new(
        "https://youtu.be/k9lGVF6jYN4",
        "The Ifa Divination System (UNESCO — patrimônio imaterial)",
        "youtube",
    )
}

fn ref_odu_yt() -> Reference {
    Reference::new(
        "https://youtu.be/yrRm_mQUPRU",
        "Odu Ifá Explained: the 16 sacred Yoruba wisdom codes",
        "youtube",
    )
}

fn ref_egbe_yt() -> Reference {
    Reference::new(
        "https://youtu.be/rIjKdOlzxWs",
        "Egbe Orun Explained: heavenly companions and destiny support",
        "youtube",
    )
}

// ─── Iorubá ────────────────────────────────────────────────────────────────────

fn yoruba_elements() -> Vec<Element> {
    vec![
        Element::new("ase", "àṣẹ", "yo")
            .at(0.0, 0.0)
            .with_pronunciation("[à.ʃɛ]")
            .with_gloss("Força vital; o poder que realiza a palavra. Também: 'assim seja' (afirmação ritual).")
            .with_concept("co://concepts/word.md")
            .with_ref(ref_ase()),
        Element::new("ori", "orí", "yo")
            .at(220.0, -40.0)
            .with_pronunciation("[ó.ɾí]")
            .with_gloss("Cabeça; intuição espiritual e destino — o núcleo interior do ser.")
            .with_ref(ref_ori()),
        Element::new("orixa", "òrìṣà", "yo")
            .at(-200.0, 80.0)
            .with_pronunciation("[ɔ̀.ɾì.ʃà]")
            .with_gloss("Divindade; força da natureza cultuada no Candomblé e na Umbanda.")
            .with_ref(ref_orisha()),
        Element::new("iemanja", "Yemọja", "yo")
            .at(-260.0, -120.0)
            .with_pronunciation("[je.mɔ.dʒa]")
            .with_gloss("Rainha das águas, mãe dos orixás e da humanidade; Iemanjá no Brasil.")
            .with_concept("co://concepts/mother.md")
            .with_ref(ref_iemanja_yt()),
        Element::new("ogum", "Ògún", "yo")
            .at(-120.0, 220.0)
            .with_pronunciation("[ò.ɡún]")
            .with_gloss("Orixá do ferro, da guerra e do trabalho; abre estradas com o facão."),
        Element::new("oxala", "Ọbàtálá", "yo")
            .at(120.0, 200.0)
            .with_pronunciation("[ɔ.ba.ta.la]")
            .with_gloss("Oxalá — pai criador, orixá da paz e da criação dos seres humanos."),
        Element::new("exu", "Èṣù", "yo")
            .at(0.0, 320.0)
            .with_pronunciation("[è.ʃù]")
            .with_gloss("Mensageiro entre orixás e humanos; guardião e abridor dos caminhos.")
            .with_ref(ref_exu_yt()),
        Element::new("iya", "ìyá", "yo")
            .at(-360.0, -40.0)
            .with_pronunciation("[ì.já]")
            .with_gloss("Mãe.")
            .with_concept("co://concepts/mother.md"),
        Element::new("ede", "èdè", "yo")
            .at(280.0, 120.0)
            .with_pronunciation("[è.dè]")
            .with_gloss("Língua, idioma, fala — como em Èdè Yorùbá.")
            .with_concept("co://concepts/word.md")
            .with_ref(ref_ede_yt()),
        Element::new("oro", "ọ̀rọ̀", "yo")
            .at(360.0, 40.0)
            .with_pronunciation("[ɔ̀.ɾɔ̀]")
            .with_gloss("Palavra, assunto, matéria da fala — a unidade, distinta de èdè (a língua-todo).")
            .with_concept("co://concepts/word.md"),
        Element::new("omi", "omi", "yo")
            .at(-380.0, 120.0)
            .with_pronunciation("[ò.mi]")
            .with_gloss("Água."),
        Element::new("ile", "ilẹ̀", "yo")
            .at(160.0, 360.0)
            .with_pronunciation("[ì.lɛ̀]")
            .with_gloss("Terra, chão, a terra-mãe."),
        Element::new("omo", "ọmọ", "yo")
            .at(-160.0, -200.0)
            .with_pronunciation("[ɔ.mɔ]")
            .with_gloss("Filho, criança, descendência."),
        // ── Complexo de Ifá (cosmologia + adivinhação) ───────────────────────
        Element::new("olodumare", "Olódùmarè", "yo")
            .at(560.0, -260.0)
            .with_pronunciation("[ò.lò.du.ma.rɛ̀]")
            .with_gloss("Ser Supremo; fonte de todo àṣẹ. O deus criador distante de quem emana a existência — não cultuado diretamente, mas através dos orixás.")
            .with_ref(ref_olodumare_yt()),
        Element::new("orunmila", "Òrúnmìlà", "yo")
            .at(560.0, 0.0)
            .with_pronunciation("[ò.ɾũ.mì.là]")
            .with_gloss("Orixá da sabedoria e da adivinhação; elérìí ìpín, a 'testemunha do destino' presente quando cada orí é escolhido. Dono do oráculo de Ifá.")
            .with_ref(ref_orunmila_yt()),
        Element::new("ifa", "Ifá", "yo")
            .at(720.0, 80.0)
            .with_pronunciation("[ì.fá]")
            .with_gloss("Sistema oracular e corpo de sabedoria revelado por Òrúnmìlà — recitado pelo babalawô. Patrimônio Imaterial da Humanidade (UNESCO).")
            .with_concept("co://concepts/word.md")
            .with_ref(ref_ifa_unesco_yt()),
        Element::new("odu", "odù", "yo")
            .at(720.0, -120.0)
            .with_pronunciation("[ò.dù]")
            .with_gloss("Os 256 signos/capítulos do corpus de Ifá; cada odù reúne versos, mitos e prescrições reveladas na adivinhação.")
            .with_concept("co://concepts/word.md")
            .with_ref(ref_odu_yt()),
        Element::new("egbe", "Egbé Òrun", "yo")
            .at(380.0, -360.0)
            .with_pronunciation("[ɛ.gbe ò.ɾũ]")
            .with_gloss("Companheiros celestiais no òrun (além-mundo); a 'turma-alma' que cada pessoa deixa ao nascer e que ampara seu destino.")
            .with_ref(ref_egbe_yt()),
    ]
}

// ─── Mbyá Guaraní ───────────────────────────────────────────────────────────────

fn mbya_elements() -> Vec<Element> {
    vec![
        Element::new("nhandereko", "nhandereko", "gn-mbya")
            .at(0.0, 0.0)
            .with_pronunciation("[ɲãn.de.ɾe.ko]")
            .with_gloss("Nosso modo de viver — cultura, costume e lei Mbyá tomados como um todo.")
            .with_ref(ref_nhandereko())
            .with_ref(ref_nhandereko_yt()),
        Element::new("teko", "teko", "gn-mbya")
            .at(-220.0, 60.0)
            .with_pronunciation("[te.ko]")
            .with_gloss("Vida, ser, modo de ser e de estar; raiz de nhandereko. 'Teko porã' = bem viver."),
        Element::new("tekoa", "tekoa", "gn-mbya")
            .at(-260.0, -100.0)
            .with_pronunciation("[te.ko.a]")
            .with_gloss("Aldeia — o lugar onde se vive segundo o nhandereko."),
        Element::new("ayvu", "ayvu", "gn-mbya")
            .at(220.0, -40.0)
            .with_pronunciation("[a.ʔɨ.vu]")
            .with_gloss("Fala-prática, alma-palavra; também nomeia a língua. Paralelo do iorubá èdè.")
            .with_concept("co://concepts/word.md"),
        Element::new("ayvu-rapyta", "ayvu rapyta", "gn-mbya")
            .at(360.0, -120.0)
            .with_pronunciation("[a.ʔɨ.vu ɾa.pɨ.ta]")
            .with_gloss("Fundamento da fala humana — o mito de origem em que Ñamandu cria a linguagem antes do mundo (Cadogan).")
            .with_concept("co://concepts/word.md")
            .with_ref(ref_ayvu_rapyta_yt())
            .with_ref(ref_ayvu_palavra_alma_yt()),
        Element::new("nhee", "nhe'ẽ", "gn-mbya")
            .at(220.0, 100.0)
            .with_pronunciation("[ɲe.ʔẽ]")
            .with_gloss("Alma-palavra; o nome-alma que vem das divindades e faz a pessoa.")
            .with_concept("co://concepts/word.md"),
        Element::new("nhanderu", "Ñamandu", "gn-mbya")
            .at(160.0, -240.0)
            .with_pronunciation("[ɲa.mãn.du]")
            .with_gloss("Pai primeiro / Nhanderu — a divindade criadora que faz a linguagem com sabedoria (kuaarara).")
            .with_ref(ref_nhamandu_yt()),
        Element::new("yvy", "yvy", "gn-mbya")
            .at(-380.0, 120.0)
            .with_pronunciation("[ɨ.vɨ]")
            .with_gloss("Terra, chão, o mundo. 'Yvy pyau' = terra nova."),
        Element::new("kaaguy", "ka'aguy", "gn-mbya")
            .at(-180.0, 220.0)
            .with_pronunciation("[ka.a.ɡwɨ]")
            .with_gloss("Mata, floresta — a Mata Atlântica como casa e sustento."),
        Element::new("ara", "ara", "gn-mbya")
            .at(40.0, 240.0)
            .with_pronunciation("[a.ɾa]")
            .with_gloss("Tempo, era, mundo, dia — a escala dos ciclos cósmicos."),
        Element::new("ara-pyau", "ara pyau", "gn-mbya")
            .at(120.0, 360.0)
            .with_pronunciation("[a.ɾa pɨ.au]")
            .with_gloss("Era nova / mundo novo — renovação cíclica, não progresso linear.")
            .with_concept("co://concepts/new-cycle.md"),
        Element::new("jaxy", "jaxy", "gn-mbya")
            .at(-40.0, -220.0)
            .with_pronunciation("[dʒa.ʃɨ]")
            .with_gloss("Lua. 'Jaxy pyau' = lua nova — mesmo morfema pyau de ara pyau."),
        Element::new("xy", "xy", "gn-mbya")
            .at(-360.0, -40.0)
            .with_pronunciation("[ʃɨ]")
            .with_gloss("Mãe.")
            .with_concept("co://concepts/mother.md"),
        Element::new("ayvu-pora", "ayvu porã", "gn-mbya")
            .at(380.0, 40.0)
            .with_pronunciation("[a.ʔɨ.vu põ.ɾã]")
            .with_gloss("Belas palavras — a fala bela e verdadeira, ideal do dizer Mbyá.")
            .with_concept("co://concepts/beautiful-words.md"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instantiate_yoruba() {
        let room = instantiate("yoruba", "r1", "alice").unwrap();
        assert_eq!(room.lang, "yo");
        assert_eq!(room.template, "yoruba");
        assert!(room.elements.iter().any(|e| e.word == "àṣẹ"));
        // todos têm língua iorubá
        assert!(room.elements.iter().all(|e| e.lang == "yo"));
    }

    #[test]
    fn instantiate_mbya() {
        let room = instantiate("mbya", "r1", "bob").unwrap();
        assert_eq!(room.lang, "gn-mbya");
        assert!(room.elements.iter().any(|e| e.word == "ayvu rapyta"));
        assert!(room.elements.iter().all(|e| e.lang == "gn-mbya"));
    }

    #[test]
    fn instantiate_blank_vazio() {
        let room = instantiate("blank", "r1", "carol").unwrap();
        assert!(room.elements.is_empty());
    }

    #[test]
    fn instantiate_desconhecido_none() {
        assert!(instantiate("fantasma", "r1", "x").is_none());
    }

    #[test]
    fn ids_de_elementos_unicos_por_template() {
        for slug in ["yoruba", "mbya"] {
            let room = instantiate(slug, "r", "o").unwrap();
            let mut ids: Vec<&str> = room.elements.iter().map(|e| e.id.as_str()).collect();
            let n = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), n, "ids duplicados em {slug}");
        }
    }

    #[test]
    fn links_referenciam_elementos_existentes() {
        // Toda relação-semente deve ligar ids de elementos que existem, e os
        // ids de link devem ser únicos — pega erro de digitação nos seeds.
        for slug in ["yoruba", "mbya"] {
            let room = instantiate(slug, "r", "o").unwrap();
            let ids: std::collections::HashSet<&str> =
                room.elements.iter().map(|e| e.id.as_str()).collect();
            let mut link_ids = std::collections::HashSet::new();
            assert!(
                !room.links.is_empty(),
                "{slug} deveria ter relações-semente"
            );
            for l in &room.links {
                assert!(
                    link_ids.insert(l.id.as_str()),
                    "link id duplicado em {slug}: {}",
                    l.id
                );
                assert!(
                    ids.contains(l.from.as_str()),
                    "{slug}: link from inexistente: {}",
                    l.from
                );
                assert!(
                    ids.contains(l.to.as_str()),
                    "{slug}: link to inexistente: {}",
                    l.to
                );
                assert_ne!(l.from, l.to, "{slug}: self-link {}", l.id);
            }
        }
    }

    #[test]
    fn summaries_incluem_tres_templates() {
        let s = template_summaries();
        assert_eq!(s.len(), 3);
        assert!(s.iter().any(|t| t.slug == "yoruba" && t.element_count > 0));
        assert!(s.iter().any(|t| t.slug == "mbya" && t.element_count > 0));
    }

    #[test]
    fn conceitos_referenciam_arquivos_existentes() {
        // Só conceitos que existem no repo comunicacao devem aparecer, para
        // não gerar wikilinks quebrados.
        let existing = ["word", "mother", "new-cycle", "beautiful-words"];
        for slug in ["yoruba", "mbya"] {
            let room = instantiate(slug, "r", "o").unwrap();
            for el in &room.elements {
                if let Some(c) = &el.concept {
                    let name = c
                        .trim_start_matches("co://concepts/")
                        .trim_end_matches(".md");
                    assert!(
                        existing.contains(&name),
                        "conceito inexistente referenciado: {c}"
                    );
                }
            }
        }
    }
}
